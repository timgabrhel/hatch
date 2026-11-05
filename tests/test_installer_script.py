"""Exercise the real shell bootstrap with disposable download stubs, never agents."""
import os
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'src/scripts/install.sh'


class InstallerScriptTests(unittest.TestCase):
    def test_all_native_routes_and_prerequisites_with_disposable_downloads(self):
        with tempfile.TemporaryDirectory(prefix='hatch-native-routes-') as temporary:
            directory = Path(temporary)
            bin_dir = directory / 'tools'
            bin_dir.mkdir()
            tmp_dir = directory / 'tmp'
            tmp_dir.mkdir()
            tools = ['sh', 'bash', 'mktemp', 'rm', 'mkdir', 'chmod', 'uname', 'tar',
                     'gzip', 'awk', 'sed', 'grep', 'head', 'cut', 'tr', 'ln', 'mv',
                     'date', 'basename', 'dirname', 'sha256sum', 'sha512sum', 'cp',
                     'cat', 'sort', 'wc', 'id', 'ldd', 'readlink', 'od', 'ls', 'sleep']
            for tool in tools:
                (bin_dir / tool).symlink_to(shutil.which(tool))
            marker = directory / 'executed.json'
            download_log = directory / 'download.json'
            payload = directory / 'payload.sh'
            payload.write_text('''/usr/bin/python3 - "$@" <<'PY'
import json, os, sys
from pathlib import Path
Path(os.environ['HATCH_TEST_MARKER']).write_text(json.dumps({
    'args': sys.argv[1:], 'prefix': os.getenv('PREFIX'),
    'path': os.environ['PATH'], 'shell': os.getenv('SHELL'),
    'codex_dir': os.getenv('CODEX_INSTALL_DIR')}))
PY
''')
            curl = bin_dir / 'curl'
            curl.write_text('''#!/usr/bin/python3
import json, os, sys, shutil
from pathlib import Path
Path(os.environ['HATCH_TEST_DOWNLOAD_LOG']).write_text(json.dumps(sys.argv[1:]))
shutil.copyfile(os.environ['HATCH_TEST_PAYLOAD'], sys.argv[-1])
sys.exit(int(os.getenv('HATCH_TEST_DOWNLOAD_EXIT', '0')))
''')
            curl.chmod(0o755)
            env = dict(os.environ, PATH=str(bin_dir), TMPDIR=str(tmp_dir),
                       HATCH_TEST_MARKER=str(marker), HATCH_TEST_PAYLOAD=str(payload),
                       HATCH_TEST_DOWNLOAD_LOG=str(download_log))
            routes = {
                'codex': 'https://chatgpt.com/codex/install.sh',
                'claude': 'https://claude.ai/install.sh',
                'copilot': 'https://gh.io/copilot-install',
                'opencode': 'https://opencode.ai/v2/install',
                'agy': 'https://antigravity.google/cli/install.sh',
                'cursor-agent': 'https://cursor.com/install',
                'grok': 'https://x.ai/cli/install.sh',
            }
            for agent, url in routes.items():
                with self.subTest(agent=agent):
                    process = subprocess.run(['/bin/sh', str(SCRIPT), agent, 'install'],
                                             env=env, capture_output=True, timeout=10)
                    self.assertEqual(process.returncode, 0, process.stderr)
                    self.assertIn(url, json.loads(download_log.read_text()))
                    result = json.loads(marker.read_text())
                    if agent == 'opencode': self.assertEqual(result['args'], ['--no-modify-path'])
                    if agent == 'agy': self.assertEqual(result['args'], ['--dir', env['HOME']+'/.local/bin'])
                    if agent == 'copilot': self.assertEqual(result['prefix'], env['HOME']+'/.local')
                    if agent == 'codex': self.assertEqual(result['codex_dir'], env['HOME']+'/.local/bin')
                    if agent == 'grok':
                        self.assertIn(env['HOME']+'/.grok/bin', result['path'].split(':'))
                        self.assertEqual(result['shell'], '/bin/sh')
                    marker.unlink()
                    process = subprocess.run(['/bin/sh', str(SCRIPT), agent, 'install'],
                                             env=dict(env, HATCH_TEST_DOWNLOAD_EXIT='22'),
                                             capture_output=True, timeout=10)
                    self.assertEqual(process.returncode, 22, process.stderr)
                    self.assertFalse(marker.exists())
                    self.assertEqual(list(tmp_dir.iterdir()), [])
            download_log.unlink()
            (bin_dir / 'bash').unlink()
            process = subprocess.run(['/bin/sh', str(SCRIPT), 'cursor-agent', 'check'],
                                     env=env, capture_output=True, timeout=10)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn(b'requires bash', process.stderr)
            self.assertFalse(download_log.exists())

    def test_failed_partial_download_is_not_executed_and_temp_is_removed(self):
        with tempfile.TemporaryDirectory(prefix='hatch-download-') as directory:
            directory = Path(directory)
            # Link read-only prerequisite tools into a controlled PATH. HOME is
            # inherited, but the script cannot reach installation after curl fails.
            tools = ['sh', 'mktemp', 'rm', 'mkdir', 'chmod', 'uname', 'tar', 'gzip',
                     'awk', 'sed', 'grep', 'head', 'cut', 'tr', 'ln', 'mv', 'date',
                     'basename', 'dirname', 'sha256sum']
            for tool in tools:
                path = shutil.which(tool)
                self.assertIsNotNone(path, tool)
                (directory / tool).symlink_to(path)
            marker = directory / 'executed'
            curl = directory / 'curl'
            curl.write_text('#!/bin/sh\nfor arg do destination=$arg; done\n'
                            'printf \'echo executed > "$HATCH_TEST_MARKER"\\n\' > "$destination"\nexit 22\n')
            curl.chmod(0o755)
            env = dict(os.environ, PATH=str(directory), TMPDIR=str(directory),
                       HATCH_TEST_MARKER=str(marker))
            before = set(directory.iterdir())
            process = subprocess.run([shutil.which('sh'), str(SCRIPT), 'codex', 'install'],
                                     env=env, capture_output=True, timeout=10)
            self.assertEqual(process.returncode, 22, process.stderr)
            self.assertFalse(marker.exists())
            self.assertEqual(set(directory.iterdir()), before)
