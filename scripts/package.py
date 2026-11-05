#!/usr/bin/env python3
"""Build a native archive with stable metadata, dependency notices and SHA-256."""
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile

root = Path(__file__).resolve().parents[1]
cargo = os.environ.get('CARGO', 'cargo')
subprocess.run([cargo, 'build', '--release', '--locked'], cwd=root, check=True)
metadata = json.loads(subprocess.check_output([cargo, 'metadata', '--locked', '--format-version', '1'], cwd=root))
package = next(p for p in metadata['packages'] if p['name'] == 'hatch-cli')
arch = {'x86_64': 'x86_64', 'aarch64': 'aarch64'}.get(platform.machine())
if platform.system() != 'Linux' or arch is None:
    raise SystemExit('Release packaging currently supports native Linux x86_64/aarch64')
name = f"hatch-{package['version']}-linux-{arch}"
dist = root / 'dist'
dist.mkdir(exist_ok=True)
files = {'hatch': root/'target/release/hatch', 'README.md': root/'README.md',
         'DESIGN.md': root/'DESIGN.md', 'scripts/install.sh': root/'scripts/install.sh',
         'tooling/package.json': root/'tooling/package.json',
         'tooling/package-lock.json': root/'tooling/package-lock.json'}
notices = []
for dependency in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
    if dependency['name'] == 'hatch-cli': continue
    notices.append(f"{dependency['name']} {dependency['version']}: {dependency.get('license', 'see source')}")
    directory = Path(dependency['manifest_path']).parent
    for path in directory.iterdir():
        if path.is_file() and path.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE')):
            files[f"licenses/{dependency['name']}-{dependency['version']}/{path.name}"] = path
archive = dist / (name + '.tar.gz')
with archive.open('wb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w') as tar:
    for target in sorted([*files, 'THIRD_PARTY_NOTICES.txt']):
        data = ('\n'.join(notices)+'\n').encode() if target == 'THIRD_PARTY_NOTICES.txt' else files[target].read_bytes()
        info = tarfile.TarInfo(name + '/' + target)
        info.size = len(data)
        info.mode = 0o755 if target in ('hatch', 'scripts/install.sh') else 0o644
        info.mtime = int(os.getenv('SOURCE_DATE_EPOCH', '0'))
        tar.addfile(info, io.BytesIO(data))
checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix(archive.suffix + '.sha256').write_text(checksum + '  ' + archive.name + '\n')
print(archive)
print(checksum)
