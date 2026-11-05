#!/usr/bin/python3
"""Disposable Docker process double. No real Docker or containers are used."""
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
with open(os.environ['HATCH_TEST_LOG'], 'a') as stream:
    stream.write(json.dumps({'args': [os.fsencode(a).hex() for a in args],
                             'pid': os.getpid(), 'agent': os.getenv('HERDR_AGENT')}) + '\n')
mode = os.getenv('HATCH_TEST_MODE', '')
if mode == 'docker-error':
    sys.stderr.write('fixture: Docker socket inaccessible\n')
    sys.exit(1)
if args[:1] == ['context']:
    print('default' if args[1] == 'show' else 'unix:///fixture/docker.sock')
elif args[:1] == ['ps']:
    sys.stdout.write('' if mode == 'empty' else 'fixture-id\n')
elif args[:1] == ['inspect']:
    if mode == 'malformed':
        print('{broken')
        sys.exit(0)
    data = json.loads(Path(os.environ['HATCH_TEST_FIXTURE']).read_text())
    target = args[-1]
    if target != 'fixture-id':
        data = [c for c in data if c['Name'].lstrip('/') == target or c['Id'] == target]
    if mode == 'single':
        data = data[:1]
    if mode == 'stopped':
        for c in data:
            c['State']['Running'] = False
    if mode == 'paused':
        for c in data:
            c['State']['Paused'] = True
    print(json.dumps(data))
elif args[:1] == ['exec']:
    container_index = next((i for i, value in enumerate(args) if len(value) == 64 and set(value) <= set('abcdef0123456789')), len(args))
    launching = '-i' in args[:container_index]
    if args[-2:] == ['login','status']:
        if os.getenv('HATCH_TEST_AUTH_MODE') == 'signed-out': sys.exit(1)
        print('Logged in using fixture credentials')
        sys.exit(0)
    if args[-2:] == ['auth','status']:
        print(json.dumps({'loggedIn': os.getenv('HATCH_TEST_AUTH_MODE') != 'signed-out', 'email': 'private@example.test'}))
        sys.exit(0)
    state = Path(os.environ['HATCH_TEST_LOG'] + '.installed')
    if 'hatch-locate' in args:
        if mode == 'local-agent':
            sys.stdout.write('found\0/home/dev/.local/bin/' + args[-1])
        elif mode == 'broken-agent':
            sys.stdout.write('found\0/usr/bin/' + args[-1])
        else:
            sys.stdout.write('missing\0/home/dev')
        sys.exit(0)
    if 'hatch-install' in args:
        if mode == 'no-prereqs':
            sys.stderr.write('Installation requires curl inside the container')
            sys.exit(1)
        if args[-1] == 'install':
            if mode == 'install-fails':
                sys.stderr.write('fixture: download failed')
                sys.exit(1)
            state.touch()
        sys.exit(0)
    if not launching and mode in ['local-agent', 'broken-agent', 'needs-install', 'install-fails', 'verify-fails', 'no-prereqs']:
        if mode == 'local-agent' and '/home/dev/.local/bin/' in args[-2]:
            print('fixture local agent')
            sys.exit(0)
        if state.exists() and mode != 'verify-fails':
            print('fixture installed agent')
            sys.exit(0)
        sys.stderr.write('fixture: cannot run executable')
        sys.exit(127)
    # Preflight has no -i; final launch always does, even when the user's tail
    # is --version. This also checks the administrative invocation verbatim.
    if not launching:
        if mode == 'stdout-error':
            print('fixture: OCI executable missing on PATH')
            sys.exit(127)
        if mode == 'missing-agent':
            sys.stderr.write('fixture: executable not found\n')
            sys.exit(127)
        if mode == 'exec-failure':
            # Force execvp itself to fail after a successful preflight.
            Path(sys.argv[0]).unlink()
        print('fixture agent 1.0')
    else:
        print(json.dumps({'stdin': '' if sys.stdin.isatty() else sys.stdin.buffer.read().hex()}))
        sys.exit(int(os.getenv('HATCH_TEST_EXIT', '0')))
else:
    sys.stderr.write('unexpected fake Docker invocation: ' + repr(args))
    sys.exit(2)
