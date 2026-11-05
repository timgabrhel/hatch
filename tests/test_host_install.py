"""Host installer checks use only a disposable prefix; cf stays untouched."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
BINARY=Path(os.environ.get('HATCH_TEST_BINARY', ROOT/'target/debug/hatch')).resolve()

class HostInstallTests(unittest.TestCase):
    def test_installs_atomically_alongside_cf_and_refuses_unrelated_hatch(self):
        with tempfile.TemporaryDirectory(prefix='hatch install ') as directory:
            prefix=Path(directory)
            (prefix/'bin').mkdir()
            cf=prefix/'bin/cf'
            cf.write_text('legacy wrapper sentinel')
            command=[str(ROOT/'scripts/install.sh'),'--prefix',str(prefix),'--bin',str(BINARY)]
            p=subprocess.run(command,capture_output=True,text=True,timeout=10)
            self.assertEqual(p.returncode,0,p.stderr)
            self.assertEqual((prefix/'bin/hatch').read_bytes(),BINARY.read_bytes())
            self.assertEqual(cf.read_text(),'legacy wrapper sentinel')
            self.assertEqual(list((prefix/'bin').glob('.hatch.*')),[])
            unrelated=prefix/'bin/hatch'
            unrelated.write_text('#!/bin/sh\necho "Unrelated CLI"\n')
            unrelated.chmod(0o755)
            original=unrelated.read_bytes()
            p=subprocess.run(command,capture_output=True,text=True,timeout=10)
            self.assertEqual(p.returncode,1)
            self.assertEqual(unrelated.read_bytes(),original)
            self.assertEqual(cf.read_text(),'legacy wrapper sentinel')
