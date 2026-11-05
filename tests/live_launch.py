"""Opt-in, read-only terminal smoke test using an existing container Python.

python3 tests/live_launch.py CONTAINER [CONTAINER_PYTHON]
No agent installations, file writes, worktrees, or container lifecycle changes.
"""
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import sys
import termios
import time

binary = Path(__file__).resolve().parents[1] / 'target/release/hatch'
container = sys.argv[1]
python = sys.argv[2] if len(sys.argv) > 2 else '/usr/bin/python3'
script = '''import os, signal, sys
signal.alarm(20)
signal.signal(signal.SIGWINCH, lambda *_: print('RESIZED', os.get_terminal_size(), flush=True))
print('READY', os.getuid(), os.getcwd(), flush=True)
try:
    while True:
        print('INPUT', repr(input()), flush=True)
except KeyboardInterrupt:
    print('INTERRUPTED', flush=True)
    sys.exit(130)
'''
pid, fd = pty.fork()
if pid == 0:
    os.execv(binary, [str(binary), '--container', container, '--agent-path', python, '--', '-c', script])
output = b''
reaped = False

def expect(needle):
    global output
    deadline = time.monotonic() + 10
    while needle not in output:
        remaining = deadline - time.monotonic()
        assert remaining > 0, output
        ready, _, _ = select.select([fd], [], [], remaining)
        assert ready, output
        output += os.read(fd, 8192)

try:
    expect(b'READY')
    assert Path(f'/proc/{pid}/exe').resolve().name == 'docker', 'Hatch remained resident'
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 31, 101, 0, 0))
    expect(b'columns=101, lines=31')
    os.write(fd, b'hello from host\n')
    expect(b"INPUT 'hello from host'")
    os.write(fd, b'\x03')
    expect(b'INTERRUPTED')
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        ended, status = os.waitpid(pid, os.WNOHANG)
        if ended:
            reaped = True
            assert os.waitstatus_to_exitcode(status) == 130, status
            break
        time.sleep(0.05)
    assert reaped, 'Docker did not exit after Ctrl-C'
    print('PASS: real Docker exec replacement, terminal resize, input, Ctrl-C, exit 130')
    print(output.decode(errors='replace'))
finally:
    if not reaped:
        os.kill(pid, signal.SIGTERM)
        os.waitpid(pid, 0)
    os.close(fd)
