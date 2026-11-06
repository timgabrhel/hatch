#!/bin/sh
# Install Hatch for this user. Never edits cf, shell profiles, or devcontainers.
set -eu
prefix=${HOME:?}/.local
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=$root/hatch
[ -f "$binary" ] || binary=$root/target/release/hatch
with_resolver=false
launch=true
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix) prefix=$2; shift 2 ;;
        --bin) binary=$2; shift 2 ;;
        --with-resolver) with_resolver=true; shift ;;
        --no-launch) launch=false; shift ;;
        *) echo "Usage: $0 [--prefix DIR] [--bin FILE] [--with-resolver] [--no-launch]" >&2; exit 2 ;;
    esac
done
case "$prefix" in /*) ;; *) echo '--prefix must be an absolute path' >&2; exit 2;; esac
[ -x "$binary" ] || { echo "Build Hatch first or supply --bin FILE" >&2; exit 1; }
if [ -e "$prefix/bin/hatch" ]; then
    case "$("$prefix/bin/hatch" --version 2>/dev/null || true)" in
        'hatch '[0-9]*) ;;
        *) echo "An unrelated command already exists at $prefix/bin/hatch; choose another --prefix." >&2; exit 1 ;;
    esac
fi
if "$with_resolver"; then
    command -v node >/dev/null || { echo 'Node.js is required for the optional Dev Container resolver.' >&2; exit 1; }
    command -v npm >/dev/null || { echo 'npm is required for the optional Dev Container resolver.' >&2; exit 1; }
    resolver=$prefix/share/hatch/resolver
    mkdir -p "$resolver"
    cp "$root/tooling/package.json" "$root/tooling/package-lock.json" "$resolver/"
    npm ci --prefix "$resolver" --no-audit --no-fund
    mkdir -p "$prefix/bin"
    if [ ! -e "$prefix/bin/devcontainer" ] && [ ! -L "$prefix/bin/devcontainer" ]; then
        ln -s "$resolver/node_modules/.bin/devcontainer" "$prefix/bin/devcontainer"
    else
        echo "Existing devcontainer command kept. Managed resolver: $resolver/node_modules/.bin/devcontainer"
    fi
fi
mkdir -p "$prefix/bin"
staged=$(mktemp "$prefix/bin/.hatch.XXXXXX")
trap 'rm -f "$staged"' EXIT
cp "$binary" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$prefix/bin/hatch"
echo "Installed $prefix/bin/hatch. Ensure $prefix/bin is on PATH. cf was not changed."
if "$launch"; then
    exec "$prefix/bin/hatch"
fi
