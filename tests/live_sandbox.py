"""Opt-in read-only Codex sandbox acceptance; uses and cleans a disposable directory.

Requires an already installed Codex CLI. Never installs it or changes permissions.
Usage: python3 tests/live_sandbox.py CONTAINER USER
"""
import os
from pathlib import Path
import subprocess
import sys

container = sys.argv[1]
user = sys.argv[2]
docker = ['docker', 'exec', '--user', user, container]
binary = Path(os.environ.get('HATCH_TEST_BINARY', Path(__file__).resolve().parents[1] / 'target/release/hatch'))
root = subprocess.check_output(docker + ['mktemp', '-d', '/tmp/hatch-sandbox.XXXXXXXX'], text=True).strip()
assert root.startswith('/tmp/hatch-sandbox.')
try:
    subprocess.run(docker + ['/usr/bin/python3', '-c', 'import pathlib,sys; pathlib.Path(sys.argv[1],"read-check").write_text("sentinel")', root], check=True)
    code = '''from pathlib import Path
import errno
assert Path("read-check").read_text() == "sentinel"
try:
    Path("write-check").write_text("disposable")
except OSError as error:
    assert error.errno in (errno.EPERM, errno.EACCES, errno.EROFS), error
    print("PASS: sandbox read succeeded and write denied")
else:
    raise SystemExit("Current sandbox profile allows writes; read-only acceptance failed")
'''
    subprocess.run([str(binary), '--container', container, '--user', user, '--metadata-only',
                    '--workspace', root, '--', 'sandbox', '--', '/usr/bin/python3', '-c', code], check=True, timeout=30)
finally:
    subprocess.run(docker + ['/usr/bin/python3', '-c', 'import shutil,sys; shutil.rmtree(sys.argv[1])', root], check=True)
