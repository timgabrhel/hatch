#!/bin/sh
# Install Hatch for this user. Shell profile changes require explicit consent.
set -eu

# Prompts use the controlling terminal so curl | sh keeps stdin for the script.
confirm() {
    [ "$no_input" = false ] || return 1
    ( : </dev/tty ) 2>/dev/null || return 1
    printf '%s [y/N] ' "$1" >/dev/tty
    IFS= read -r answer </dev/tty || return 1
    case "$answer" in y|Y|yes|YES) return 0 ;; *) return 1 ;; esac
}

configure_path() {
    case ":${PATH:-}:" in *:"$install_dir":*) return ;; esac
    printf '%s is not on PATH.\n' "$install_dir" >&2
    case "${SHELL##*/}" in
        bash) profile=$HOME/.bashrc ;;
        zsh) profile=${ZDOTDIR:-$HOME}/.zshrc ;;
        sh|dash|ksh|'') profile=$HOME/.profile ;;
        *) printf 'Run %s/hatch directly, or add its directory to your shell PATH.\n' "$install_dir" >&2; return ;;
    esac
    # Single-quote the literal path; never evaluate user-supplied path text.
    quoted=$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")
    line="export PATH='$quoted':\"\$PATH\""
    if [ -f "$profile" ] && grep -Fqx "$line" "$profile"; then
        printf 'PATH is already configured in %s. Open a new terminal to use hatch.\n' "$profile" >&2
    elif confirm "Add Hatch to PATH in $profile?"; then
        printf '\n# Hatch\n%s\n' "$line" >> "$profile"
        printf 'Updated %s. Open a new terminal to use hatch.\n' "$profile" >&2
    else
        printf 'Run %s/hatch directly. Your shell profile was not changed.\n' "$install_dir" >&2
    fi
    # This also makes the optional resolver available to a local post-install launch.
    PATH=$install_dir:$PATH
    export PATH
}

# Parse the complete script before executing when invoked through curl | sh.
main() {
    prefix=${HOME:?}/.local
    install_dir=
    no_input=false
    update=false
    SHELL=${SHELL:-}
    root=
    binary=
    if [ -f "$0" ]; then
        root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
        binary=$root/hatch
        [ -f "$binary" ] || binary=$root/target/release/hatch
    fi
    download_dir=
    staged=
    trap 'rm -f -- "${staged:-}"; if [ -n "${download_dir:-}" ]; then rm -rf -- "$download_dir"; fi' EXIT
    with_resolver=auto
    launch=true
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --prefix|--bin|--install-dir)
                [ "$#" -ge 2 ] && [ -n "$2" ] || { echo "$1 needs a value" >&2; exit 2; }
                case "$1" in --prefix) prefix=$2 ;; --bin) binary=$2 ;; --install-dir) install_dir=$2 ;; esac
                shift 2 ;;
            --with-resolver) with_resolver=true; shift ;;
            --without-resolver) with_resolver=false; shift ;;
            --no-input) no_input=true; shift ;;
            --update) update=true; shift ;;
            --no-launch) launch=false; shift ;;
            *) echo "Usage: $0 [--prefix DIR] [--bin FILE] [--with-resolver | --without-resolver] [--no-launch] [--no-input]" >&2; exit 2 ;;
        esac
    done
    case "$prefix" in /*) ;; *) echo '--prefix must be an absolute path' >&2; exit 2;; esac
    install_dir=${install_dir:-$prefix/bin}
    case "$install_dir" in /*) ;; *) echo '--install-dir must be absolute' >&2; exit 2 ;; esac
    if "$update"; then
        binary=
        with_resolver=false
        launch=false
        no_input=true
    fi
    if [ -z "$binary" ]; then
        case "$(uname -s)/$(uname -m)" in
            Linux/x86_64) arch=x86_64 ;;
            Linux/aarch64|Linux/arm64) arch=aarch64 ;;
            *) echo 'Hatch releases support Linux x86_64 and ARM64.' >&2; exit 1 ;;
        esac
        for tool in curl tar sha256sum; do
            command -v "$tool" >/dev/null || { echo "$tool is required." >&2; exit 1; }
        done
        releases=https://github.com/timgabrhel/hatch/releases
        release_url=$(curl -fsSL --connect-timeout 15 --max-time 300 --retry 2 --proto '=https' --proto-redir '=https' -o /dev/null -w '%{url_effective}' "$releases/latest")
        tag=${release_url##*/}
        version=${tag#v}
        case "$tag" in
            v[0-9]*) ;;
            *) echo 'Could not resolve the latest Hatch release.' >&2; exit 1 ;;
        esac
        case "$version" in
            *[!0-9.]*|'') echo 'Invalid release version.' >&2; exit 1 ;;
        esac
        if "$update"; then
            installed=$("$install_dir/hatch" --version)
            case "$installed" in 'hatch '[0-9]*) ;; *) echo 'Not a Hatch installation.' >&2; exit 1 ;; esac
            current=${installed#hatch }
            newest=$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -n 1)
            if [ "$newest" = "$current" ]; then
                printf 'Hatch %s is already up to date (latest release: %s).\n' "$current" "$version"
                return
            fi
        fi
        name=hatch-$version-linux-$arch
        download_dir=$(mktemp -d)
        for file in "$name.tar.gz" "$name.tar.gz.sha256"; do
            curl -fsSL --connect-timeout 15 --max-time 300 --retry 2 --proto '=https' --proto-redir '=https' "$releases/download/$tag/$file" -o "$download_dir/$file"
        done
        read -r expected recorded < "$download_dir/$name.tar.gz.sha256"
        [ "$recorded" = "$name.tar.gz" ] && [ "${#expected}" -eq 64 ] || { echo 'Invalid archive checksum.' >&2; exit 1; }
        case "$expected" in *[!0-9a-f]*) echo 'Invalid archive checksum.' >&2; exit 1 ;; esac
        actual=$(sha256sum "$download_dir/$name.tar.gz"); actual=${actual%% *}
        [ "$actual" = "$expected" ] || { echo 'Archive checksum mismatch.' >&2; exit 1; }
        tar -xzf "$download_dir/$name.tar.gz" -C "$download_dir" "$name/hatch" "$name/tooling/package.json" "$name/tooling/package-lock.json"
        root=$download_dir/$name
        binary=$root/hatch
        [ -f "$binary" ] && [ ! -L "$binary" ] && [ -x "$binary" ] || { echo 'Archive has no regular Hatch executable.' >&2; exit 1; }
        [ "$("$binary" --version)" = "hatch $version" ] || { echo 'Downloaded executable version mismatch.' >&2; exit 1; }
        # stdin contains this script, not terminal input for Hatch's menus.
        launch=false
    fi
    [ -x "$binary" ] || { echo "Build Hatch first or supply --bin FILE" >&2; exit 1; }
    if [ -e "$install_dir/hatch" ]; then
        case "$("$install_dir/hatch" --version 2>/dev/null || true)" in
            'hatch '[0-9]*) ;;
            *) echo "An unrelated command already exists at $install_dir/hatch; choose another --prefix." >&2; exit 1 ;;
        esac
    fi
    if [ "$with_resolver" = auto ]; then
        with_resolver=false
        if ! command -v devcontainer >/dev/null 2>&1 && [ ! -x "$install_dir/devcontainer" ]; then
            if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
                if confirm 'Install the Dev Container CLI for config resolution and container startup?'; then
                    with_resolver=true
                fi
            else
                echo 'Dev Container CLI is unavailable. Container startup needs Node.js/npm and the resolver.' >&2
            fi
        fi
    fi
    if "$with_resolver"; then
        command -v node >/dev/null || { echo 'Node.js is required for the optional Dev Container resolver.' >&2; exit 1; }
        command -v npm >/dev/null || { echo 'npm is required for the optional Dev Container resolver.' >&2; exit 1; }
        resolver=$prefix/share/hatch/resolver
        mkdir -p "$resolver"
        cp "$root/tooling/package.json" "$root/tooling/package-lock.json" "$resolver/"
        npm ci --prefix "$resolver" --no-audit --no-fund
        mkdir -p "$install_dir"
        if [ ! -e "$install_dir/devcontainer" ] && [ ! -L "$install_dir/devcontainer" ]; then
            ln -s "$resolver/node_modules/.bin/devcontainer" "$install_dir/devcontainer"
        else
            echo "Existing devcontainer command kept. Managed resolver: $resolver/node_modules/.bin/devcontainer"
        fi
    fi
    mkdir -p "$install_dir"
    staged=$(mktemp "$install_dir/.hatch.XXXXXX")
    cp "$binary" "$staged"
    chmod 755 "$staged"
    mv -f "$staged" "$install_dir/hatch"
    echo "Installed $install_dir/hatch."
    if ! "$update"; then configure_path; fi
    if [ -n "$download_dir" ]; then
        rm -rf -- "$download_dir"
        download_dir=
    fi
    if "$launch"; then
        exec "$install_dir/hatch"
    fi
}

main "$@"
