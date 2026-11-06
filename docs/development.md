# Development, testing, and packaging

[← Back to README](../README.md)

## Build from source

Development requires Git, current stable Rust, and standard Linux shell tools.
Docker is needed to use Hatch against real containers; the default tests use fixtures.

```sh
git clone https://github.com/timgabrhel/hatch.git
cd hatch
cargo build --release --locked
./scripts/install.sh
```

Run the installer on the host. It offers missing PATH and resolver setup, then opens
Hatch. Use `--no-launch` to install only. Resolver setup requires Node.js/npm.

If your shell selects an older system Cargo, run `source "$HOME/.cargo/env"` before
building. To update a local build, rebuild and run `./scripts/install.sh` again.

## Install a devcontainer build

Hatch's devcontainer bind-mounts the source checkout, so build output is already
available on the host. No Docker copy step is needed.

Build inside the container:

```sh
cargo build --release --locked
```

Then run this **on the host**, from the same checkout:

```sh
./scripts/install.sh --bin target/release/hatch
```

The executable must be compatible with the host's architecture and Linux libraries.

## Development container

`hatch.code-workspace` is an empty VS Code workspace placeholder (`folders: []`).
Selecting it in Hatch uses the detected project folder without editing the file.

The repository's `.devcontainer/devcontainer.json` uses Microsoft's Rust Bookworm
image and Node.js 22. Its setup hook installs rustfmt and clippy, fetches locked Cargo
dependencies, and installs the pinned Dev Container CLI under `tooling/node_modules`.

The workspace is `/workspaces/hatch`, using the checkout's bind mount and `vscode` as
the development user.
Its `runArgs` sets `--name=hatch-dev` for newly created containers. Existing containers
are not renamed automatically.

Open the folder with VS Code's **Reopen in Container**, or from the Docker host:

```sh
hatch up --config .devcontainer/devcontainer.json --host-workspace . --agent codex
```

## Host authentication mounts

This project's config explicitly shares the host's file-based Codex, Claude, and GitHub
authentication. It mounts `~/.codex/auth.json`, `~/.claude/.credentials.json`, and
`~/.config/gh/hosts.yml` read-only directly at their usual paths under `/home/vscode`.
All three host files must exist and be readable by the development user's
mapped UID.

GitHub CLI is installed and configured as Git's HTTPS credential helper.
The creation hook gives `vscode` ownership of the mount parent directories, which
Docker may create as root. It does not change the mounted credential files.

No credentials are copied. Agents cannot save refreshed tokens to these read-only
files. If a host login replaces a credential file, recreate the container to remount
the new file. Host files are never written; histories and other host settings are not
mounted. Keychain-only logins and SSH authentication are not forwarded by this config.

These mounts take effect when creating/recreating the devcontainer, not when attaching
to an existing one. This is an explicit project configuration, not a Hatch-wide default.
There are no agent installations or Docker socket mounts in this config.

Install missing agents through Hatch's final review after startup. Default tests use
fixtures and need no Docker daemon; run live discovery from the Docker host.

Dependencies need network access on initial setup. The image and Feature major tags
track supported updates rather than pinning immutable digests.

## Build and test

Run the default checks on the host or inside the development container:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

To test the release binary too:

```sh
cargo build --release --locked
HATCH_TEST_BINARY=target/release/hatch cargo test --locked --tests
```

See [AGENTS.md](../AGENTS.md) for contributor instructions and test boundaries.

`cargo test` runs unit tests, process integration tests, and real PTY menu tests.
The Rust `hatch-test-fixture` binary doubles as Docker, Herdr, the resolver, and curl
inside disposable test directories; it is not shipped in release archives. Tests
isolate HOME and PATH and never contact real agents, worktrees, or containers.
Native bootstrap tests never download agents.

## Live checks

Live tests are ignored by default. Opt in to each separately with an existing container
and intended development user. The terminal test requires `/usr/bin/env`, `/bin/sh`,
`id`, and `stty` inside it. The sandbox test requires an already installed Codex and
standard shell tools, writes only in its own temporary directory, and cleans it up:

```sh
HATCH_LIVE_CONTAINER=CONTAINER HATCH_LIVE_USER=USER cargo test --locked --test live terminal_launch -- --ignored
HATCH_LIVE_CONTAINER=CONTAINER HATCH_LIVE_USER=USER cargo test --locked --test live sandbox_read_only -- --ignored
```

## Packaging

```sh
cargo run --locked --example package
```

The packaging example uses GNU tar, gzip, and sha256sum. It produces a native Linux
x86_64 or aarch64 archive in `dist/`, with stable tar/gzip metadata and a SHA-256 checksum.
Set `CARGO=/path/to/cargo` if needed.

The archive includes the binary, installer, resolver lockfile, README, `docs/` guides,
`DESIGN.md`, `AGENTS.md`, and dependency license notices. These are native GNU/Linux
builds, not universal static binaries.

The **Hatch — Validate code** workflow runs formatting, Clippy, Rust and release-helper
tests, and a build. Release builds reuse that validation before packaging.

## Releases

Run **Hatch — Prepare release** on `main`, choosing `patch` (default), `minor`, `major`,
or an exact version. It updates `Cargo.toml` and `Cargo.lock` together, opens a release
PR, waits for its checks, and merges the validated commit if repository rules allow.
Approve the bot PR's workflows when GitHub requests it; the workflow handles the rest.

The merged version is tagged immutably and the build workflow is dispatched explicitly.
Version-file changes merged through an ordinary PR use the same tagging path.
All checks, Linux x86_64/ARM64 builds, release-binary tests, and isolated archive
installation tests must pass before a draft release is published. Published assets
are never replaced; retries can resume a draft.

Pull requests and manual builds also produce downloadable Actions artifacts.
Manual publication requires selecting a version tag and enabling `publish`.
Release archives include checksums and the installer used by `hatch update`.

Release management is adapted from Tars. Its Python helpers use only the standard
library (Python 3.11+ on CI); Hatch itself remains a Rust CLI. To test those helpers:

```sh
python3 -m unittest discover -s tools/release -p 'test_*.py'
```

See [DESIGN.md](../DESIGN.md) for implementation status and limitations.
