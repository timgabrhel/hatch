"""Run after cargo build: python3 -m unittest discover -s tests -v.

Process tests use disposable executable fixtures. No containers, installations,
credentials, real worktrees, Herdr sessions, or user configuration are changed.
"""
import json
import os
import pty
import select
import time
from pathlib import Path
import shutil
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get('HATCH_TEST_BINARY', ROOT / 'target/debug/hatch')).resolve()


class HatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BINARY.is_file():
            raise RuntimeError('Build Hatch first with cargo build; executable missing: ' + str(BINARY))

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='hatch-test-')
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        docker = self.directory / 'docker'
        shutil.copyfile(ROOT / 'tests/fake_docker.py', docker)
        docker.chmod(0o755)
        self.log = self.directory / 'calls.jsonl'
        self.env = dict(os.environ, PATH=str(self.directory),
                        HATCH_TEST_LOG=str(self.log), HATCH_PLAIN="1", HERDR_ENV="0",
                        HATCH_TEST_FIXTURE=str(ROOT / 'tests/fixtures/containers.json'))
        for name in ['HATCH_TEST_MODE', 'HATCH_TEST_EXIT', 'HERDR_AGENT']:
            self.env.pop(name, None)

    def run_hatch(self, *args, data=b'', mode='', status=0):
        self.env['HATCH_TEST_MODE'] = mode
        process = subprocess.Popen([os.fsencode(BINARY), *[os.fsencode(a) for a in args]],
                                   env=self.env, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        stdout, stderr = process.communicate(data, timeout=10)
        self.assertEqual(process.returncode, status, stderr.decode(errors='replace'))
        return process.pid, stdout, stderr

    def calls(self):
        return [dict(row, args=[bytes.fromhex(a) for a in row['args']])
                for row in map(json.loads, self.log.read_text().splitlines())] if self.log.exists() else []

    def test_help_version_and_errors_do_not_need_docker(self):
        (self.directory / 'docker').unlink()
        self.run_hatch('--help')
        self.run_hatch('--version')
        self.run_hatch('--agent', 'wrong', status=2)
        self.run_hatch('resume', status=2)
        self.assertEqual(self.calls(), [])

    def test_discovery_filters_ordinary_containers_and_never_executes(self):
        _, stdout, _ = self.run_hatch('containers', '--json')
        self.assertEqual([c['name'] for c in json.loads(stdout)], ['alpha-dev', 'beta-compose'])
        self.assertEqual([c['args'][0] for c in self.calls()], [b'ps', b'inspect'])

    def test_resolve_layouts(self):
        for name, workspace, user, host in [
            ('alpha-dev', '/workspaces/alpha', 'developer', '/projects/alpha'),
            ('beta-compose', '/srv/beta', '1001:1001', None),
        ]:
            _, stdout, _ = self.run_hatch('--container', name, 'inspect')
            context = json.loads(stdout)
            self.assertEqual((context['workspace'], context['user'], context['daemon_host_path']),
                             (workspace, user, host))
        self.assertTrue(all(c['args'][0] == b'inspect' for c in self.calls()))

    def test_exact_forwarding_stdin_exit_status_and_exec_pid(self):
        tail = [b'exec', b'', b'white space', b"'\";$HOME`echo x`\n", b'\xff', b'--', b'--help']
        self.env['HATCH_TEST_EXIT'] = '23'
        pid, stdout, _ = self.run_hatch('--container', 'alpha-dev', '--no-input', '--', *tail,
                                       data=b'input\x00data\n', status=23)
        calls = self.calls()
        final = calls[-1]
        self.assertEqual(final['pid'], pid, 'Hatch must replace itself with Docker')
        self.assertEqual(final['agent'], 'codex')
        self.assertEqual(final['args'], [b'exec', b'--user', b'developer', b'--workdir',
                         b'/workspaces/alpha', b'-i', b'a' * 64, b'codex', *tail])
        self.assertEqual(json.loads(stdout)['stdin'], b'input\x00data\n'.hex())
        self.assertEqual(calls[-2]['args'][-2:], [b'codex', b'--version'])

    def test_claude_admin_command_has_no_injected_flags(self):
        self.run_hatch('--container', 'alpha-dev', '--agent', 'claude', '--', 'login')
        final = self.calls()[-1]
        self.assertEqual(final['args'][-2:], [b'claude', b'login'])
        self.assertEqual(final['agent'], 'claude')

    def test_explicit_ordinary_container_and_executable(self):
        self.run_hatch('--container', 'ordinary', '--workspace', '/some dir', '--user', '42',
                       '--agent-path', '/home/dev/.local/bin/codex')
        self.assertEqual(self.calls()[-1]['args'], [b'exec', b'--user', b'42', b'--workdir',
                         b'/some dir', b'-i', b'c' * 64, b'/home/dev/.local/bin/codex'])

    def test_noninteractive_ambiguity_and_empty_discovery(self):
        for mode, expected in [('', b'multiple devcontainers'), ('empty', b'no running devcontainers')]:
            _, _, error = self.run_hatch('--no-input', mode=mode, status=1)
            self.assertIn(expected, b' '.join(error.split()))
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_failure_cases_never_launch(self):
        for mode, expected in [('docker-error', b'socket inaccessible'),
                               ('malformed', b'invalid Docker inspect'),
                               ('stopped', b'stopped or paused'),
                               ('paused', b'stopped or paused'),
                               ('missing-agent', b'container-local installation'),
                               ('stdout-error', b'container-local installation')]:
            _, _, error = self.run_hatch('--container', 'alpha-dev', mode=mode, status=1)
            self.assertIn(expected, b' '.join(error.split()))
        self.assertFalse(any(b'-i' in c['args'][:6] for c in self.calls()))

    def test_failed_exec_is_reported(self):
        _, _, error = self.run_hatch('--container', 'alpha-dev', mode='exec-failure', status=1)
        self.assertIn(b'could not replace Hatch', error)

    def test_menu_never_consumes_piped_input(self):
        self.run_hatch('menu', data=b'1\n', status=1)
        self.assertEqual(self.calls(), [])

    def test_read_only_command_rejects_agent_tail(self):
        self.run_hatch('inspect', '--', 'login', status=1)
        self.assertEqual(self.calls(), [])

    def test_terminal_allocation_and_no_tty_override(self):
        for no_tty in [False, True]:
            master, slave = pty.openpty()
            try:
                args = [str(BINARY), '--container', 'alpha-dev', '--no-input']
                if no_tty:
                    args.append('--no-tty')
                process = subprocess.Popen(args, env=self.env, stdin=slave, stdout=slave, stderr=slave)
                os.close(slave)
                slave = None
                try:
                    self.assertEqual(process.wait(timeout=10), 0)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
                self.assertEqual(b'-t' in self.calls()[-1]['args'], not no_tty)
            finally:
                os.close(master)
                if slave is not None:
                    os.close(slave)

    def test_prompt_cancellation_has_no_exec(self):
        master, slave = pty.openpty()
        process = None
        try:
            process = subprocess.Popen([str(BINARY), 'menu'], env=self.env,
                                       stdin=slave, stdout=slave, stderr=slave)
            os.close(slave)
            slave = None
            output = b''
            deadline = time.monotonic() + 10
            while b'q cancels' not in output:
                remaining = deadline - time.monotonic()
                self.assertGreater(remaining, 0, output)
                readable, _, _ = select.select([master], [], [], remaining)
                self.assertTrue(readable, output)
                output += os.read(master, 4096)
            os.write(master, b'q\n')
            self.assertEqual(process.wait(timeout=10), 1)
            self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            if slave is not None:
                os.close(slave)

    def test_missing_agent_install_and_verify_for_both_agents(self):
        for agent in ['codex', 'claude']:
            Path(str(self.log) + '.installed').unlink(missing_ok=True)
            self.run_hatch('--container', 'alpha-dev', '--agent', agent, '--install', '--',
                           'login', mode='needs-install')
            final = self.calls()[-1]['args']
            self.assertEqual(final[-2:], [('/home/dev/.local/bin/' + agent).encode(), b'login'])
            installs = [c for c in self.calls() if b'hatch-install' in c['args']]
            self.assertEqual(installs[-1]['args'][-1], b'install')
            self.assertIn(b'developer', installs[-1]['args'])

    def test_local_agent_is_found_without_installation(self):
        self.run_hatch('--container', 'alpha-dev', mode='local-agent')
        self.assertEqual(self.calls()[-1]['args'][-1], b'/home/dev/.local/bin/codex')
        self.assertFalse(any(b'hatch-install' in c['args'] for c in self.calls()))

    def test_broken_agent_is_not_reinstalled(self):
        self.run_hatch('--container', 'alpha-dev', '--install', mode='broken-agent', status=1)
        self.assertFalse(any(b'hatch-install' in c['args'] for c in self.calls()))

    def test_installer_failures_do_not_launch(self):
        for mode in ['no-prereqs', 'install-fails', 'verify-fails']:
            Path(str(self.log) + '.installed').unlink(missing_ok=True)
            self.run_hatch('--container', 'alpha-dev', '--install', mode=mode, status=1)
        self.assertFalse(any(b'-i' in c['args'][:6] for c in self.calls()))

    def test_noninteractive_install_requires_opt_in(self):
        _, _, error = self.run_hatch('--container', 'alpha-dev', '--no-input', mode='needs-install', status=1)
        self.assertIn(b'--install', error)
        self.assertFalse(any(b'hatch-install' in c['args'] for c in self.calls()))

    def test_install_is_not_an_upgrade(self):
        self.run_hatch('--container', 'alpha-dev', '--install')
        self.assertFalse(any(b'hatch-install' in c['args'] for c in self.calls()))

    def test_install_prompt_accept_and_decline(self):
        for answer in [b'n\n', b'y\n']:
            Path(str(self.log) + '.installed').unlink(missing_ok=True)
            self.log.unlink(missing_ok=True)
            self.env['HATCH_TEST_MODE'] = 'needs-install'
            master, slave = pty.openpty()
            process = None
            try:
                process = subprocess.Popen([str(BINARY), '--container', 'alpha-dev', '--agent', 'codex'], env=self.env,
                                           stdin=slave, stdout=slave, stderr=slave)
                os.close(slave)
                slave = None
                output = b''
                deadline = time.monotonic() + 10
                while b'[y/N]' not in output:
                    remaining = deadline - time.monotonic()
                    self.assertGreater(remaining, 0, output)
                    readable, _, _ = select.select([master], [], [], remaining)
                    self.assertTrue(readable, output)
                    output += os.read(master, 4096)
                os.write(master, answer)
                self.assertEqual(process.wait(timeout=10), 0 if answer == b'y\n' else 1)
                mutations = [c for c in self.calls() if b'hatch-install' in c['args'] and c['args'][-1] == b'install']
                self.assertEqual(len(mutations), 1 if answer == b'y\n' else 0)
            finally:
                if process is not None and process.poll() is None:
                    process.kill()
                    process.wait()
                os.close(master)
                if slave is not None:
                    os.close(slave)

    def test_invalid_install_options_do_not_contact_docker(self):
        self.run_hatch('--install', '--agent-path', '/usr/bin/codex', status=2)
        self.run_hatch('--install', 'inspect', status=1)
        self.assertEqual(self.calls(), [])

    def menu_session(self, arguments, answers, expected_status=0, mode='', command='menu'):
        self.env['HATCH_TEST_MODE'] = mode
        master, slave = pty.openpty()
        process = None
        transcript = b''
        try:
            process = subprocess.Popen([str(BINARY), *([command] if command else []), *arguments], env=self.env,
                                       stdin=slave, stdout=slave, stderr=slave)
            os.close(slave)
            slave = None
            for answer in answers:
                marker = b'(q cancels): '
                if isinstance(answer, tuple): marker, answer = answer
                output = b''
                deadline = time.monotonic() + 10
                while marker not in output:
                    remaining = deadline - time.monotonic()
                    self.assertGreater(remaining, 0, output)
                    ready, _, _ = select.select([master], [], [], remaining)
                    self.assertTrue(ready, output)
                    output += os.read(master, 4096)
                transcript += output
                os.write(master, answer + b'\n')
            self.assertEqual(process.wait(timeout=10), expected_status, transcript)
            while select.select([master], [], [], 0.1)[0]:
                try:
                    chunk = os.read(master, 8192)
                except OSError:
                    break
                if not chunk: break
                transcript += chunk
            return transcript
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            if slave is not None:
                os.close(slave)

    def test_menu_container_then_agent_and_argument_forwarding(self):
        transcript = self.menu_session(['--', 'resume', 'a b', ''], [b'2', b'2'])
        self.assertLess(transcript.index(b'Select a container'), transcript.index(b'Select an agent'))
        final = self.calls()[-1]
        self.assertEqual(final['agent'], 'claude')
        self.assertEqual(final['args'][-5:], [b'b' * 64, b'claude', b'resume', b'a b', b''])
        self.assertIn(b'/srv/beta', final['args'])

    def test_menu_default_agent_and_single_container_still_prompt(self):
        transcript = self.menu_session([], [b'1', b''], mode='single')
        self.assertIn(b'Select a container', transcript)
        self.assertIn(b'Codex', transcript)
        self.assertEqual(self.calls()[-1]['agent'], 'codex')

    def test_menu_explicit_container_and_agent_default(self):
        transcript = self.menu_session(['--container', 'alpha-dev', '--agent', 'claude'], [b''])
        self.assertNotIn(b'Select a container', transcript)
        self.assertNotIn(b'Back to containers', transcript)
        self.assertIn(b'Enter = 2', transcript)
        self.assertEqual(self.calls()[-1]['agent'], 'claude')

    def test_menu_back_then_select_another_container(self):
        transcript = self.menu_session([], [b'1', b'8', b'2', b'1'])
        self.assertEqual(transcript.count(b'Select a container'), 2)
        self.assertEqual(self.calls()[-1]['args'][-2:], [b'b' * 64, b'codex'])

    def test_menu_cancel_agent_does_not_probe_or_install(self):
        self.menu_session(['--install'], [b'1', b'q'], expected_status=1)
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def resolver(self):
        resolver = self.directory / 'devcontainer'
        shutil.copyfile(ROOT / 'tests/fake_devcontainer.py', resolver)
        resolver.chmod(0o755)
        config = self.directory / 'devcontainer.json'
        config.write_text('{ /* resolver owns JSONC parsing */ }')
        self.env['HATCH_TEST_RESOLVER_LOG'] = str(self.directory / 'resolver.log')
        return ['--container', 'alpha-dev', '--config', str(config), '--host-workspace', str(self.directory)]

    def test_resolved_configuration_and_redacted_inspect(self):
        args = self.resolver()
        _, output, _ = self.run_hatch(*args, 'inspect')
        context = json.loads(output)
        self.assertEqual(context['workspace'], '/custom/project')
        self.assertEqual(context['user'], '2000')
        self.assertEqual(context['configuration_source'], 'devcontainer-cli')
        self.assertNotIn(b'super-secret', output)
        self.assertEqual(context['remote_env_names'], ['HATCH_REMOVED', 'HATCH_TOKEN'])
        calls = json.loads((self.directory / 'resolver.log').read_text().splitlines()[0])
        self.assertIn('--include-merged-configuration', calls)
        self.assertEqual(calls[calls.index('--container-id') + 1], 'a' * 64)
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def provision_fixture(self, matching=False):
        args = self.resolver()[2:]
        fixture = json.loads((ROOT / 'tests/fixtures/containers.json').read_text())
        if matching:
            fixture[0]['Config']['Labels']['devcontainer.config_file'] = args[1]
            fixture[0]['Config']['Labels']['devcontainer.local_folder'] = args[3]
        path = self.directory / 'containers.json'
        path.write_text(json.dumps(fixture))
        self.env['HATCH_TEST_FIXTURE'] = str(path)
        return args

    def resolver_calls(self):
        path = self.directory / 'resolver.log'
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_config_identity_reuses_running_without_up(self):
        args = self.provision_fixture(matching=True)
        for command in [[], ['up'], ['inspect']]:
            self.run_hatch(*command, *args, '--no-input')
        self.assertTrue(all(c[0] == 'read-configuration' for c in self.resolver_calls()))
        self.assertEqual(self.calls()[-1]['args'][-1], b'fixture-id')

    def test_config_readonly_and_unattended_never_create(self):
        args = self.provision_fixture()
        for command in [[], ['inspect'], ['doctor']]:
            _, _, error = self.run_hatch(*command, *args, '--no-input', status=1)
            self.assertIn(b'hatch up', error)
        _, output, _ = self.run_hatch('containers', '--json', *args)
        self.assertEqual(json.loads(output), [])
        self.assertEqual(self.resolver_calls(), [])
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_up_forwards_identity_input_and_agent_arguments(self):
        args = self.provision_fixture()
        pid, output, _ = self.run_hatch('up', *args, '--no-input', '--agent', 'claude',
                                      '--', b'', b'a b', b'$HOME', b'\xff', data=b'agent stdin')
        up = self.resolver_calls()[0]
        self.assertEqual(up, ['up', *args[:2], '--workspace-folder', args[3],
                             '--id-label', 'devcontainer.local_folder='+args[3],
                             '--id-label', 'devcontainer.config_file='+args[1], '--no-lockfile'])
        self.assertEqual(self.calls()[-1]['args'][-5:], [b'claude', b'', b'a b', b'$HOME', b'\xff'])
        self.assertEqual(self.calls()[-1]['pid'], pid)
        self.assertEqual(json.loads(output)['stdin'], b'agent stdin'.hex())

    def test_up_failure_and_bad_results_do_not_launch(self):
        args = self.provision_fixture()
        for mode in ['error', 'invalid', 'unsuccessful', 'wrong-target']:
            self.env['HATCH_TEST_UP_MODE'] = mode
            _, output, error = self.run_hatch('up', *args, status=1)
            self.assertNotIn(b'secret-invalid-result', output + error)
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_up_argument_validation_and_missing_tool(self):
        self.run_hatch('up', status=1)
        args = self.provision_fixture()
        self.run_hatch('up', *args, '--container', 'alpha-dev', status=1)
        self.assertEqual(self.calls(), [])
        (self.directory / 'devcontainer').unlink()
        _, _, error = self.run_hatch('up', *args, status=1)
        self.assertIn(b'could not start Dev Container CLI', error)

    def test_stopped_match_is_started_but_ambiguous_running_matches_fail(self):
        args = self.provision_fixture(matching=True)
        path = Path(self.env['HATCH_TEST_FIXTURE'])
        fixture = json.loads(path.read_text())
        fixture[0]['State']['Running'] = False
        path.write_text(json.dumps(fixture))
        self.run_hatch('up', *args, '--no-input')
        self.assertEqual(self.resolver_calls()[0][0], 'up')
        fixture = json.loads(path.read_text())
        duplicate = json.loads(json.dumps(fixture[0]))
        duplicate['Id'] = 'd' * 64
        duplicate['Name'] = '/duplicate'
        fixture.append(duplicate)
        path.write_text(json.dumps(fixture))
        self.log.unlink()
        (self.directory / 'resolver.log').unlink()
        self.run_hatch('up', *args, '--no-input', status=1)
        self.assertEqual(self.resolver_calls(), [])
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_matching_workspace_with_different_profile_never_attaches(self):
        args = self.provision_fixture(matching=True)
        path = Path(self.env['HATCH_TEST_FIXTURE'])
        fixture = json.loads(path.read_text())
        fixture[0]['Config']['Labels']['devcontainer.config_file'] += '.different'
        path.write_text(json.dumps(fixture))
        self.run_hatch(*args, '--no-input', status=1)
        self.assertEqual(self.resolver_calls(), [])
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_config_missing_match_can_cancel_or_create_interactively(self):
        args = self.provision_fixture()
        self.menu_session(args, [b''], expected_status=1)
        self.assertEqual(self.resolver_calls(), [])
        self.menu_session(args, [b'2', b'2'])
        self.assertEqual(self.resolver_calls()[0][0], 'up')
        self.assertEqual(self.calls()[-1]['args'][-1], b'claude')

    def test_resolved_env_launch_and_explicit_override(self):
        args = self.resolver()
        self.run_hatch(*args, '--user', '42', '--workspace', '/override', '--', 'login')
        final = self.calls()[-1]['args']
        self.assertIn(b'42', final)
        self.assertIn(b'/override', final)
        self.assertIn(b'HATCH_TOKEN=super-secret-config-value', final)
        self.assertEqual(final[-5:], [b'env', b'-u', b'HATCH_REMOVED', b'codex', b'login'])

    def test_resolver_failures_do_not_leak_or_launch(self):
        args = self.resolver()
        for mode in ['error', 'invalid', 'compose', 'unresolved']:
            self.env['HATCH_TEST_RESOLVER_MODE'] = mode
            _, output, error = self.run_hatch(*args, status=1)
            self.assertNotIn(b'super-secret', output + error)
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_require_config_and_metadata_only(self):
        self.run_hatch('--container', 'alpha-dev', '--require-config', 'inspect', status=1)
        _, output, _ = self.run_hatch('--container', 'alpha-dev', '--metadata-only', 'inspect')
        self.assertEqual(json.loads(output)['configuration_source'], 'docker-metadata')
        self.assertEqual(json.loads(output)['warnings'], [])

    def herdr_fixture(self):
        herdr = self.directory / 'herdr'
        shutil.copyfile(ROOT / 'tests/fake_herdr.py', herdr)
        herdr.chmod(0o755)
        self.env['HATCH_TEST_HERDR_LOG'] = str(self.directory / 'herdr.log')
        self.env['HERDR_ENV'] = '1'
        socket = self.directory / 'herdr.sock'
        socket.touch()
        self.env['HERDR_SOCKET_PATH'] = str(socket)
        self.env['XDG_STATE_HOME'] = str(self.directory / 'state')
        self.env.pop('DOCKER_HOST', None)
        self.env.pop('DOCKER_CONTEXT', None)
        fixture = json.loads((ROOT / 'tests/fixtures/containers.json').read_text())
        fixture[0]['Mounts'][0]['Source'] = str(self.directory)
        fixture[0]['Config']['Labels']['devcontainer.local_folder'] = str(self.directory)
        path = self.directory / 'containers.json'
        path.write_text(json.dumps(fixture))
        self.env['HATCH_TEST_FIXTURE'] = str(path)

    def herdr_calls(self):
        return [json.loads(line) for line in (self.directory / 'herdr.log').read_text().splitlines()]

    def test_herdr_explicit_handoff_creates_fresh_workspaces(self):
        self.herdr_fixture()
        tail = ['resume', "a b'$HOME;\n", '']
        self.run_hatch('--container', 'alpha-dev', '--agent', 'claude', '--herdr', '--', *tail)
        self.run_hatch('--container', 'alpha-dev', '--agent', 'codex', '--herdr')
        calls = self.herdr_calls()
        self.assertEqual(sum(c[:2] == ['workspace', 'create'] for c in calls), 2)
        self.assertEqual(sum(c[:2] == ['tab', 'create'] for c in calls), 0)
        run = next(c for c in calls if c[:2] == ['pane', 'run'])
        forwarded = shlex.split(run[3])
        self.assertEqual(forwarded[-len(tail):], tail)
        self.assertIn('a' * 64, forwarded)
        self.assertEqual(forwarded[forwarded.index('--agent') + 1], 'claude')
        self.assertNotIn('--herdr', forwarded)
        self.assertIn('--here', forwarded)
        self.assertIn('DOCKER_CONTEXT=default', forwarded)
        self.assertFalse(any(b'-i' in c['args'][:6] for c in self.calls()))
        self.assertTrue(any(c['args'][-1] == b'--version' for c in self.calls()))

    def test_herdr_failure_does_not_retry_or_launch_locally(self):
        self.herdr_fixture()
        self.env['HATCH_TEST_HERDR_MODE'] = 'run-fails'
        _, _, error = self.run_hatch('--container', 'alpha-dev', '--herdr', status=1)
        self.assertIn(b'may have received', error)
        self.assertEqual(sum(c[:2] == ['pane', 'run'] for c in self.herdr_calls()), 1)
        self.assertFalse(any(b'-i' in c['args'][:6] for c in self.calls()))
        self.assertTrue(any(c['args'][-1] == b'--version' for c in self.calls()))

    def test_herdr_unavailable_falls_back(self):
        self.herdr_fixture()
        self.env['HATCH_TEST_HERDR_MODE'] = 'unavailable'
        self.run_hatch('--container', 'alpha-dev', '--herdr')
        self.assertEqual(self.calls()[-1]['args'][-1], b'codex')
        self.assertEqual(self.herdr_calls(), [['workspace', 'list']])

    def test_herdr_rejects_non_utf8_before_creating_anything(self):
        self.herdr_fixture()
        self.run_hatch('--container', 'alpha-dev', '--herdr', '--', b'\xff', status=1)
        self.assertTrue(all(c == ['workspace', 'list'] for c in self.herdr_calls()))

    def test_arrow_menu_and_terminal_restoration(self):
        self.env.pop('HATCH_PLAIN', None)
        self.env['TERM'] = 'xterm-256color'
        self.env['HERDR_ENV'] = '0'
        master, slave = pty.openpty()
        import termios
        original = termios.tcgetattr(slave)
        process = subprocess.Popen([str(BINARY), 'menu'], env=self.env,
                                   stdin=slave, stdout=slave, stderr=slave)
        transcript = b''
        try:
            for marker, keys in [(b'Select a container', b'\x1b[B\r'), (b'Select an agent', b'\x1b[B\r')]:
                chunk = b''
                deadline = time.monotonic() + 10
                while marker not in chunk:
                    remaining = deadline - time.monotonic()
                    self.assertGreater(remaining, 0, chunk)
                    ready, _, _ = select.select([master], [], [], remaining)
                    self.assertTrue(ready, chunk)
                    chunk += os.read(master, 8192)
                transcript += chunk
                os.write(master, keys)
            self.assertEqual(process.wait(timeout=10), 0, transcript)
            self.assertEqual(self.calls()[-1]['args'][-2:], [b'b' * 64, b'claude'])
            self.assertEqual(termios.tcgetattr(slave), original)
            self.assertIn(b'| |__   __ _| |_ ___| |__', transcript)
        finally:
            if process.poll() is None: process.kill(); process.wait()
            os.close(master)
            os.close(slave)

    def test_arrow_escape_back_then_cancel_restores_terminal(self):
        self.env.pop('HATCH_PLAIN', None)
        self.env.update(TERM='xterm-256color', HERDR_ENV='0')
        master, slave = pty.openpty()
        import termios
        original = termios.tcgetattr(slave)
        process = subprocess.Popen([str(BINARY), 'menu'], env=self.env,
                                   stdin=slave, stdout=slave, stderr=slave)
        transcript = b''
        try:
            for marker, keys in [(b'Select a container', b'\r'), (b'Select an agent', b'\x1b'),
                                 (b'Select a container', b'\x1b')]:
                output = b''
                deadline = time.monotonic() + 10
                while marker not in output:
                    remaining = deadline - time.monotonic()
                    self.assertGreater(remaining, 0, output)
                    ready, _, _ = select.select([master], [], [], remaining)
                    self.assertTrue(ready, output)
                    output += os.read(master, 8192)
                transcript += output
                os.write(master, keys)
            self.assertEqual(process.wait(timeout=10), 1)
            self.assertEqual(transcript.count(b'Esc cancel/back'), 1)
            self.assertEqual(termios.tcgetattr(slave), original)
            self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))
        finally:
            if process.poll() is None: process.kill(); process.wait()
            os.close(master)
            os.close(slave)

    def test_doctor_reports_status_without_credentials_or_installation(self):
        _, output, _ = self.run_hatch('--container', 'alpha-dev', 'doctor')
        result = json.loads(output)
        self.assertEqual([a['authentication'] for a in result['agents'][:2]], ['authenticated', 'authenticated'])
        self.assertEqual(len(result['agents']), 7)
        self.assertTrue(all(a['authentication'].startswith('not checked') for a in result['agents'][2:]))
        self.assertNotIn(b'private@example.test', output)
        self.assertNotIn(b'fixture credentials', output)
        self.assertFalse(any(b'hatch-install' in c['args'] for c in self.calls()))

    def test_config_discovery_without_profile_label(self):
        self.resolver()
        fixture = json.loads((ROOT / 'tests/fixtures/containers.json').read_text())
        labels = fixture[0]['Config']['Labels']
        labels.pop('devcontainer.config_file')
        labels['devcontainer.local_folder'] = str(self.directory)
        data = self.directory / 'containers.json'
        data.write_text(json.dumps(fixture))
        self.env['HATCH_TEST_FIXTURE'] = str(data)
        folder = self.directory / '.devcontainer'
        folder.mkdir()
        default = folder / 'devcontainer.json'
        default.write_text('{}')
        _, output, _ = self.run_hatch('--container', 'alpha-dev', 'inspect')
        self.assertEqual(json.loads(output)['config_file'], str(default))
        default.unlink()
        for name in ['one', 'two']:
            (folder / name).mkdir()
            (folder / name / 'devcontainer.json').write_text('{}')
        _, _, error = self.run_hatch('--container', 'alpha-dev', 'inspect', status=1)
        self.assertIn(b'Multiple devcontainer profiles', error)

    def test_automatic_herdr_menu_has_no_destination_question(self):
        self.herdr_fixture()
        transcript = self.menu_session([], [b'1', b'6'])
        self.assertNotIn(b'Open session', transcript)
        self.assertEqual(sum(c[:2] == ['workspace', 'create'] for c in self.herdr_calls()), 1)
        command = next(c[3] for c in self.herdr_calls() if c[:2] == ['pane', 'run'])
        forwarded = shlex.split(command)
        self.assertEqual(forwarded[forwarded.index('--agent')+1], 'cursor')
        self.assertIn('--here', forwarded)
        self.assertFalse(any(b'-i' in c['args'][:6] for c in self.calls()))
        self.assertTrue(any(c['args'][-1] == b'--version' for c in self.calls()))
        # Execute the handoff under the same Herdr environment: no recursion.
        before = len(self.herdr_calls())
        child = subprocess.run(['/usr/bin/env', *forwarded[1:]], env=self.env,
                               input=b'', capture_output=True, timeout=10)
        self.assertEqual(child.returncode, 0, child.stderr)
        self.assertEqual(len(self.herdr_calls()), before)
        self.assertEqual(self.calls()[-1]['args'][-1], b'cursor-agent')

    def test_here_and_piped_input_stay_in_current_terminal(self):
        self.herdr_fixture()
        self.menu_session(['--here'], [b'1', b'1'])
        self.assertFalse((self.directory / 'herdr.log').exists())
        _, output, _ = self.run_hatch('--container', 'alpha-dev', data=b'piped text')
        self.assertEqual(json.loads(output)['stdin'], b'piped text'.hex())
        self.assertFalse((self.directory / 'herdr.log').exists())

    def test_simple_help_and_advanced_options_need_no_docker(self):
        (self.directory / 'docker').unlink()
        _, simple, _ = self.run_hatch('--help')
        self.assertNotIn(b'--no-tty', simple)
        self.assertIn(b'hatch options', simple)
        _, advanced, _ = self.run_hatch('options')
        self.assertIn(b'--no-tty', advanced)
        self.assertIn(b'Terminal features (TTY) are automatic', advanced)
        _, agents, _ = self.run_hatch('agents')
        for agent in ['copilot', 'opencode', 'antigravity', 'cursor', 'grok']:
            self.assertIn(agent.encode(), agents)
        self.assertEqual(self.calls(), [])

    def test_default_is_guided_even_for_a_single_container_and_flags_skip_steps(self):
        transcript = self.menu_session([], [b'1', b'2'], command='', mode='single')
        self.assertIn(b'Select a container', transcript)
        self.assertIn(b'Select an agent', transcript)
        self.assertEqual(self.calls()[-1]['args'][-1], b'claude')
        transcript = self.menu_session(['--agent', 'grok'], [b'1'], command='', mode='single')
        self.assertNotIn(b'Select an agent', transcript)
        self.assertEqual(self.calls()[-1]['args'][-1], b'grok')
        self.menu_session(['--container', 'alpha-dev', '--agent', 'cursor'], [], command='')
        self.assertEqual(self.calls()[-1]['args'][-1], b'cursor-agent')
        self.run_hatch('--no-input', mode='single', status=1)

    def test_terminal_diagnostics_are_readable_and_json_remains_available(self):
        transcript = self.menu_session(['--container', 'alpha-dev'], [], command='doctor')
        self.assertIn(b'Agent checks', transcript)
        self.assertIn(b'LOGIN', transcript)
        self.assertNotIn(b'"remote_env_names"', transcript)
        transcript = self.menu_session(['--container', 'alpha-dev'], [], command='inspect')
        self.assertIn(b'Launch context', transcript)
        self.assertIn(b'hatch inspect --json', transcript)
        _, output, _ = self.run_hatch('doctor', '--json', '--container', 'alpha-dev')
        self.assertEqual(len(json.loads(output)['agents']), 7)

    def test_doctor_icons_distinguish_available_authenticated_and_unknown(self):
        self.env.pop('HATCH_PLAIN', None)
        self.env.update(TERM='xterm-256color', NO_COLOR='1')
        transcript = self.menu_session(['--container', 'alpha-dev'], [], command='doctor')
        self.assertIn('✓ Available'.encode(), transcript)
        self.assertEqual(transcript.count('✓ Authenticated'.encode()), 2)
        self.assertEqual(transcript.count(b'? Not checked'), 5)
        self.env['HATCH_TEST_AUTH_MODE'] = 'signed-out'
        transcript = self.menu_session(['--container', 'alpha-dev'], [], command='doctor')
        self.assertNotIn('✓ Authenticated'.encode(), transcript)
        self.assertEqual(transcript.count(b'? Not verified'), 2)
        self.env['HATCH_PLAIN'] = '1'
        transcript = self.menu_session(['--container', 'alpha-dev'], [], command='doctor', mode='missing-agent')
        self.assertEqual(transcript.count(b'[x] Missing/unusable'), 7)
        self.assertNotIn('[ok] Authenticated'.encode(), transcript)

    def test_container_browser_accepts_config_and_detects_workspace_file(self):
        args = self.provision_fixture(matching=True)
        workspace = self.directory / 'project.code-workspace'
        workspace.write_text('{ /* comment */ "folders": [{"path": ".",},], }')
        transcript = self.menu_session([], [b'3', (b'devcontainer.json path', args[1].encode()),
                                           b'1', b'2'], command='containers')
        self.assertIn(b'Use devcontainer.json', transcript)
        self.assertIn(b'project.code-workspace', transcript)
        self.assertLess(transcript.index(b'Select a workspace'), transcript.index(b'Select an agent'))
        self.assertEqual(self.calls()[-1]['args'][-1], b'claude')
        self.assertTrue(all(c[0] == 'read-configuration' for c in self.resolver_calls()))

    def test_first_run_without_containers_can_enter_config_or_cancel(self):
        transcript = self.menu_session([], [b'1', (b'devcontainer.json path', b''), b'q'],
                                       command='', mode='empty', expected_status=1)
        self.assertIn(b'Use devcontainer.json', transcript)
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_manual_workspace_file_with_multiple_roots_and_spaces(self):
        args = self.provision_fixture(matching=True)
        external = self.directory / 'workspace files'
        external.mkdir()
        file = external / 'two roots.code-workspace'
        file.write_text('{"folders":[{"name":"Parent","path":".."},{"name":"Nested","path":"."}]}')
        transcript = self.menu_session([], [b'3', (b'devcontainer.json path', args[1].encode()),
                  b'1', (b'Workspace file or folder path', ('"'+str(file)+'"').encode()),
                  b'1', b'1'])
        self.assertIn(b'Select a project folder', transcript)
        self.assertEqual(self.calls()[-1]['args'][-1], b'codex')
        self.assertEqual(self.resolver_calls()[0][-1], str(self.directory))

    def test_workspace_back_and_invalid_path_never_launch_or_build(self):
        args = self.provision_fixture()
        self.menu_session([], [b'3', (b'devcontainer.json path', b'/missing/devcontainer.json'),
                              b'3', (b'devcontainer.json path', args[1].encode()), b'2', b'q'],
                          expected_status=1)
        self.assertEqual(self.resolver_calls(), [])
        self.assertFalse(any(c['args'][0] == b'exec' for c in self.calls()))

    def test_workspace_file_also_works_as_explicit_host_workspace(self):
        args = self.provision_fixture(matching=True)
        workspace = self.directory / 'project.code-workspace'
        workspace.write_text('{"folders":[{"path":"."}]}')
        args[-1] = str(workspace)
        _, output, _ = self.run_hatch(*args, 'inspect')
        self.assertEqual(json.loads(output)['host_workspace'], str(self.directory))

    def test_install_consent_happens_before_any_herdr_call(self):
        for answer in [b'n\n', b'y\n']:
            with self.subTest(answer=answer):
                self.herdr_fixture()
                (self.directory / 'herdr.log').unlink(missing_ok=True)
                Path(str(self.log) + '.installed').unlink(missing_ok=True)
                self.env['HATCH_TEST_MODE'] = 'needs-install'
                master, slave = pty.openpty()
                process = subprocess.Popen([str(BINARY), '--container', 'alpha-dev', '--agent', 'codex'],
                                           env=self.env, stdin=slave, stdout=slave, stderr=slave)
                os.close(slave)
                try:
                    output = b''
                    deadline = time.monotonic()+10
                    while b'[y/N]' not in output:
                        remaining = deadline-time.monotonic()
                        self.assertGreater(remaining, 0, output)
                        ready, _, _ = select.select([master], [], [], remaining)
                        self.assertTrue(ready, output)
                        output += os.read(master, 8192)
                    self.assertIn(b'Your session', output)
                    self.assertIn(b'Agent not installed', output)
                    self.assertFalse((self.directory / 'herdr.log').exists())
                    os.write(master, answer)
                    self.assertEqual(process.wait(timeout=10), 0 if answer == b'y\n' else 1)
                    if answer == b'n\n':
                        self.assertFalse((self.directory / 'herdr.log').exists())
                    else:
                        command = next(c[3] for c in self.herdr_calls() if c[:2] == ['pane', 'run'])
                        argv = shlex.split(command)
                        self.assertIn('--no-input', argv)
                        self.assertNotIn('--install', argv)
                        self.assertEqual(argv[argv.index('--agent-path')+1], '/home/dev/.local/bin/codex')
                finally:
                    if process.poll() is None: process.kill(); process.wait()
                    os.close(master)

    def test_failed_install_does_not_open_herdr(self):
        self.herdr_fixture()
        for mode in ['install-fails', 'no-prereqs', 'verify-fails', 'broken-agent']:
            Path(str(self.log) + '.installed').unlink(missing_ok=True)
            self.run_hatch('--container', 'alpha-dev', '--herdr', '--install', mode=mode, status=1)
            self.assertFalse((self.directory / 'herdr.log').exists())

    def test_all_agents_forward_exact_arguments_and_install_paths(self):
        for name, executable, relative in [
            ('codex', 'codex', '.local/bin/codex'),
            ('claude', 'claude', '.local/bin/claude'),
            ('copilot', 'copilot', '.local/bin/copilot'),
            ('opencode', 'opencode', '.opencode/bin/opencode'),
            ('antigravity', 'agy', '.local/bin/agy'),
            ('cursor', 'cursor-agent', '.local/bin/cursor-agent'),
            ('grok', 'grok', '.grok/bin/grok')]:
            with self.subTest(agent=name):
                tail = [b'', b'a b', b'$HOME;whoami', b'\xff']
                self.run_hatch('--container', 'alpha-dev', '--agent', name, '--', *tail)
                self.assertEqual(self.calls()[-1]['args'][-5:], [executable.encode(), *tail])
                Path(str(self.log) + '.installed').unlink(missing_ok=True)
                self.run_hatch('--container', 'alpha-dev', '--agent', name, '--install',
                               '--', *tail, mode='needs-install')
                self.assertEqual(self.calls()[-1]['args'][-5:], [('/home/dev/'+relative).encode(), *tail])

    def test_invalid_paths_fail_before_docker(self):
        for path in ['relative', '/work/../etc', '${containerWorkspaceFolder}']:
            self.run_hatch('--workspace', path, status=1)
        self.run_hatch('--agent-path', 'codex', status=1)
        self.assertEqual(self.calls(), [])


if __name__ == '__main__':
    unittest.main()
