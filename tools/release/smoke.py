"""Verify the native archive and install it in an isolated directory."""
import hashlib
import os
import platform
import tomllib
from pathlib import Path
import subprocess
import tempfile


def main():
    version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
    arch = {"x86_64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}[platform.machine()]
    archive = Path(f"dist/hatch-{version}-linux-{arch}.tar.gz").resolve()
    expected, name = Path(f"{archive}.sha256").read_text().split()
    if name != archive.name or hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise ValueError("Archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix="hatch-release-") as directory:
        root = Path(directory)
        env = dict(os.environ, HOME=str(root), PATH="/usr/bin:/bin")
        subprocess.run(["tar", "-xzf", str(archive), "-C", directory], check=True)
        package = root / archive.name.removesuffix(".tar.gz")
        subprocess.run([
            "sh", str(package / "scripts/install.sh"), "--no-input", "--no-launch", "--without-resolver"
        ], cwd=root, env=env, check=True)
        executable = root / ".local/bin/hatch"
        version = archive.name.removeprefix("hatch-").split("-linux-")[0]
        reported = subprocess.check_output([str(executable), "--version"], cwd=root, env=env, text=True)
        if reported.strip() != f"hatch {version}":
            raise ValueError("Installed version does not match archive")
        subprocess.run([str(executable), "update", "--help"], cwd=root, env=env, check=True)


if __name__ == "__main__":
    main()
