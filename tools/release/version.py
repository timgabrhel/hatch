"""Prepare stable release versions and validate merged version bumps before tagging."""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STABLE = re.compile(r"v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def parse_version(value: str) -> tuple[int, int, int]:
    match = STABLE.fullmatch(value)
    if not match:
        raise ValueError("Use a stable version such as 0.2.0 or v0.2.0 (no prerelease suffix).")
    return tuple(int(part) for part in match.groups())


def format_version(value: tuple[int, int, int]) -> str:
    return ".".join(str(part) for part in value)


def select_version(current: str, tags: list[str], bump: str, explicit: str = "") -> str:
    baseline = max([parse_version(current), *[parse_version(tag) for tag in tags if STABLE.fullmatch(tag)]])
    if explicit:
        selected = parse_version(explicit)
    elif bump == "major":
        selected = (baseline[0] + 1, 0, 0)
    elif bump == "minor":
        selected = (baseline[0], baseline[1] + 1, 0)
    elif bump == "patch":
        selected = (baseline[0], baseline[1], baseline[2] + 1)
    else:
        raise ValueError("Bump must be patch, minor, or major.")
    if selected <= baseline:
        raise ValueError("The release version must exceed both the project version and existing stable tags.")
    return format_version(selected)


def project_version(root: Path) -> str:
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    parse_version(version)
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    packages = [package for package in lock["package"] if package["name"] == "hatch-cli"]
    if len(packages) != 1 or packages[0]["version"] != version:
        raise ValueError("Cargo.lock must match the package version.")
    return version


def write_version(root: Path, current: str, selected: str) -> None:
    # Update only Hatch's own package stanza, leaving dependency versions untouched.
    changes = {}
    for relative, pattern in [
        ("Cargo.toml", r'(\[package\]\n(?:(?!\[).)*?version = ")' + re.escape(current) + r'(")'),
        ("Cargo.lock", r'(\[\[package\]\]\nname = "hatch-cli"\nversion = ")' + re.escape(current) + r'(")'),
    ]:
        path = root / relative
        text, count = re.subn(pattern, lambda m: m[1] + selected + m[2], path.read_text(), flags=re.S)
        if count != 1:
            raise ValueError(f"Expected exactly one Hatch version assignment in {relative}.")
        changes[path] = text
    for path, text in changes.items():
        path.write_text(text)


def merged_tag(current: str, previous: str, tags: list[str]) -> str | None:
    if current == previous:
        return None
    if parse_version(current) <= parse_version(previous):
        raise ValueError("Merged release version must increase.")
    tag = f"v{current}"
    if any(parse_version(other) > parse_version(current) for other in tags if STABLE.fullmatch(other)):
        raise ValueError("A newer stable tag already exists; refusing to publish an older release.")
    return tag


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def output(name: str, value: str) -> None:
    print(f"{name}={value}")
    if path := os.environ.get("GITHUB_OUTPUT"):
        with Path(path).open("a") as stream:
            stream.write(f"{name}={value}\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["prepare", "merged", "check"])
    args = parser.parse_args()
    current = project_version(ROOT)
    if args.mode == "check":
        tag = os.environ.get("RELEASE_TAG", "")
        if tag and tag != f"v{current}":
            raise ValueError("Release tag must match Cargo.toml and Cargo.lock.")
        print(f"hatch {current}")
        return
    tags = git("tag", "--list", "v*").splitlines()
    if args.mode == "prepare":
        selected = select_version(
            current, tags, os.environ.get("BUMP", "patch"), os.environ.get("VERSION", "")
        )
        write_version(ROOT, current, selected)
        output("tag", f"v{selected}")
    else:
        before = os.environ["BEFORE_SHA"]
        if not re.fullmatch(r"[0-9a-f]{40}", before) or before == "0" * 40:
            raise ValueError("Expected an existing main commit as the previous revision.")
        previous = tomllib.loads(git("show", f"{before}:Cargo.toml"))["package"]["version"]
        tag = merged_tag(current, previous, tags)
        if tag:
            output("tag", tag)


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        raise SystemExit(str(error)) from None
