# Hatch

```text
   _           _       _
  | |__   __ _| |_ ___| |__
  | '_ \ / _` | __/ __| '_ \
  | | | | (_| | || (__| | | |
  |_| |_|\__,_|\__\___|_| |_|
```

Launch coding agents inside devcontainers, from your host terminal.
Codex is the default; choose Claude with `--agent claude`. Hatch is a synchronous Rust
CLI: it resolves the environment, prepares the agent, then replaces itself with Docker.
No Hatch daemon, background polling, or resident wrapper.

**Worktrees are intentionally deferred.** Hatch does not create/remove worktrees,
force container rebuilds, or replace the installed `cf`. Explicit startup runs project lifecycle hooks.

## Start here

```sh
hatch                      # Guided container → workspace (for config) → agent flow
hatch menu                 # Always show the agent menu, even with --agent
hatch --agent claude
hatch --here               # Stay in this terminal, even in Herdr
hatch agents               # Supported agents
hatch options              # Advanced settings explained
hatch inspect              # Resolution sources; environment values are hidden
hatch doctor               # Agent/authentication/persistence diagnostics
hatch containers           # Browse interactively, including a pasted config path
hatch containers --json    # Read-only inventory
```

Running `hatch` interactively always asks which container to use, even when only one
is running. It then asks for an agent. `--container` or `--config` supplies the target;
`--agent` skips the agent question. `hatch menu` deliberately retains that question,
using `--agent` as the default. A custom `--agent-path` skips agent selection.

```sh
# Skip the menus when you know what you want:
hatch --container my-devcontainer --agent claude
# Scripts must identify their target; no implicit single-container launch:
hatch --container my-devcontainer --agent codex --no-input -- exec 'Run tests'
```

The container list includes **Use devcontainer.json…**, even with no running containers.
Paste a local config path, then choose its workspace. Hatch looks for `.code-workspace`
files beside the config and in the inferred project root, and offers a detected project
folder for standard config layouts. You can always paste a workspace file or folder.
Workspace files support JSON comments, trailing commas, and relative folder paths. A
multi-folder file prompts for the project folder used by the Dev Container CLI. It does
not run editor tasks or open all folders. Remote URI entries and variable-based folder
paths require you to choose a concrete local folder instead. Missing/invalid paths are
reported so you can correct them; blank path input goes back. `~/` and quoted pasted
paths work without shell evaluation.

`--host-workspace` also accepts a folder or a single-folder `.code-workspace` file;
for a multi-folder file unattended, pass the intended folder explicitly. Interactive
`hatch containers` opens this same browser. `--json`, `--no-input`, or nonterminal use
keeps it a read-only inventory.

Menus use arrow keys, Enter and Esc/Back, with color, paging and terminal restoration.
Navigation guidance appears once. The session summary identifies container, workspace,
user, agent and destination before preflight. The stylized lowercase wordmark falls
back to HATCH on narrow terminals. Notices and errors use short blocks and wrapped text.
`--plain`, `TERM=dumb`, or `HATCH_PLAIN=1` select numbered questions; `NO_COLOR` disables
color. No full-screen interface is used. `inspect` and `doctor` show readable terminal
summaries; `--json` or piped output provides complete machine-readable details.

Everyday `--help` shows the main choices. `hatch options` lists all settings with
explanations. You normally do not need the terminal settings:

- **TTY** means interactive terminal features: arrow keys, colors, resizing and Ctrl-C.
  Hatch detects these automatically; `--no-tty` is an override for scripts.
- **Prompts** are Hatch's selection/installation questions. `--no-input` fails when
  a choice is needed; it does not disable the coding agent's own questions.
- `--plain` uses numbered questions instead of arrow-key selection.

## Inventory versus inspection

- `hatch containers --json` returns an **array** of running devcontainers discovered
  through Docker labels. Ordinary service containers are excluded unless explicitly
  selected with `--container`. `docker ps` lists all running Docker containers.
- `hatch inspect` resolves **one** container's workspace, user, and config. Its JSON
  is an **object**. If only one eligible container exists, inspection selects it
  automatically; with several it asks, or needs `--container` unattended.
- Bare `hatch` always shows the container picker in an interactive terminal, even
  with one candidate. It never hardcodes RAIC. For scripts, supply the target explicitly.

`hatch doctor` uses a compact status table: ✓ available/authenticated, ✗ missing or
unusable, and ? unchecked/unverified. A working executable does not imply a login.
Unknown status never gets a green check. `--plain` uses `[ok]`, `[x]`, and `[?]`, while
`NO_COLOR` removes color without hiding status symbols. JSON remains unchanged.

## Develop Hatch in its own devcontainer

The repository's `.devcontainer/devcontainer.json` uses Microsoft's Rust Bookworm
image, Python 3 at `/usr/bin/python3`, and Node.js 22. Its setup hook ensures rustfmt
and clippy, fetches locked Cargo dependencies, and installs the pinned Dev Container
CLI under `tooling/node_modules`. The workspace is `/workspaces/hatch`, owned by the
checkout's bind mount, with `vscode` as the development user.

Open the folder with VS Code's **Reopen in Container**, or from the Docker host:

```sh
hatch up --config .devcontainer/devcontainer.json --host-workspace . --agent codex
```

There are no host credential/home mounts, agent installations, or Docker socket mounts
in this config. Agents can be installed by Hatch's normal prompt after startup. Default
Rust/Python tests use fixtures and need no Docker daemon. For live discovery, run Hatch
from the Docker host. Dependencies require network access on initial setup. The image
and Feature major tags track supported updates rather than pinning immutable digests.

Inside the development container:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
python3 -m unittest discover -s tests -v
```

See [AGENTS.md](AGENTS.md) for contributor/agent instructions and test boundaries.

## Build and install

Use **current stable Rust**, Python 3 for tests, and Docker on the host. The optional
full configuration resolver requires host Node.js/npm; neither is needed for the Rust
launcher itself or metadata-only mode.

```sh
cargo build --release --locked
scripts/install.sh --with-resolver
# Installs ~/.local/bin/hatch and the pinned official Dev Container CLI.
# No shell profiles, cf, devcontainer config, or container agents are changed.
```

If your PATH still selects an old distro Cargo, use `source "$HOME/.cargo/env"` first.
The installer accepts `--prefix DIR` and `--bin FILE`. Omit `--with-resolver` if a
`devcontainer` CLI already exists on PATH or you only need metadata resolution.
The managed resolver is pinned by `tooling/package-lock.json`; an existing
`devcontainer` command is never overwritten. Select another with `--devcontainer-cli PATH`.

## Launch and exact argument forwarding

```sh
hatch --container my-devcontainer -- resume --last
hatch --container my-devcontainer -- 'Explain this repository'
hatch --container my-devcontainer -- login --device-auth
hatch --agent claude --container my-devcontainer -- -p 'Explain this repository'
hatch --no-input --container my-devcontainer -- exec 'Run tests'
hatch --no-tty --container my-devcontainer -- exec 'Read stdin'
```

Everything after the first `--` is passed as separate OS-string arguments. Hatch adds
no agent flags, resumes nothing implicitly, and does not translate flags between agents.
Direct launch preserves empty arguments, shell metacharacters, non-UTF-8 bytes, stdin,
and agent exit status. Docker gets `-i` for piped input and `-t` only when stdin/stdout
are terminals and `--no-tty` is absent. `HERDR_AGENT` identifies the host Docker process.

## Dynamic devcontainer configuration

```sh
hatch --config /path/project/.devcontainer/devcontainer.json
hatch menu --config /path/project/.devcontainer/linux/devcontainer.json
# Explicit unattended start/create, then launch:
hatch up --config /path/project/.devcontainer/devcontainer.json --no-input
```

Matching uses both config and workspace paths. A running match is reused directly,
without rebuilding or rerunning lifecycle hooks. If no running match exists, an
interactive launch offers to start/create one (default Cancel); unattended launches
require `up`. The official Dev Container CLI handles stopped containers, image pulls,
Dockerfile builds, Features, and Compose startup. It runs the project's normal lifecycle
hooks, including host `initializeCommand`. Build/hook output streams to stderr and may
include whatever the project's scripts print. Hatch never requests container removal
or a forced rebuild. Failed startup leaves any created resources intact for diagnosis;
it does not launch an agent or automatically retry.

Standard `.devcontainer/devcontainer.json`, `.devcontainer/PROFILE/devcontainer.json`,
and `.devcontainer.json` locations infer the workspace. For other layouts, specify
`--host-workspace /path/project`; remote Docker always requires it. Source paths are
canonicalized. Profiles with missing identity labels need explicit `--container` to
attach; Hatch does not guess based on names. Multiple running matches prompt or fail
in unattended mode. `inspect`, `doctor`, and read-only inventory never start containers.

`up` requires `--config` and does not accept `--container`. It uses `--no-lockfile`
to avoid generating a Dev Container lockfile. Hatch does not edit devcontainer.json;
project hooks retain their own normal behavior. There is no editor extension host or
automatic `forwardPorts` tunnel; Docker-published ports and explicit project networking
still apply. Existing running containers attach without `postAttachCommand`.

There are no RAIC-specific names, users, paths, or setup commands in runtime code.
Discovery uses running containers' devcontainer labels, mounts, and configured user.
Ordinary containers work with explicit `--container`, `--workspace`, and `--user`.

Once a running container is selected, Hatch resolves configuration with:

```text
devcontainer read-configuration --container-id ID --config FILE \
  --workspace-folder FOLDER --include-merged-configuration
```

The official resolver handles JSONC, substitutions, local config, and image/feature
metadata. Hatch applies the resulting workspaceFolder, remoteUser and remoteEnv,
and checks Compose service identity against the selected container. The source
config path comes from the container label, or standard config locations under a
verified local workspace. Multiple profiles require a choice or explicit `--config`.

`--workspace` and `--user` override resolved values. `inspect` exposes the actual
config file, local workspace, sources, warnings, environment **names**, and
userEnvProbe policy; it never invokes shell startup scripts or reveals env values.
Resolver failures hide raw logs because those can contain credentials. Malformed
available config is an error, not a silent fallback.

Launch and `doctor` apply userEnvProbe (`none`, loginShell, interactiveShell, or
loginInteractiveShell; default loginInteractiveShell for resolved configs). Hatch
detects the user's passwd shell and home, probes the environment, then applies
remoteEnv overrides. Supported probe shells are sh/bash/dash/ash/zsh/ksh. Shell
startup scripts execute as that container user; project lifecycle hooks do not.
Unsupported/missing probe prerequisites produce an error; use userEnvProbe:none
and remoteEnv for containers without a supported shell. Setting remoteEnv to null
uses container `env -u` to remove the variable before exec. No host authentication
or history is automatically copied; config-declared localEnv substitutions and
existing container mounts continue to apply.

When the CLI or source config is unavailable, Hatch reports metadata-only fallback:
workspace from a local-folder label mapped through a bind mount, then non-root Docker
WorkingDir; user from remoteUser metadata, Docker Config.User, then UID 0. Override
image defaults when needed. Use `--require-config` to fail rather than fall back,
or `--metadata-only` to intentionally skip source resolution and shell probing.

For a remote Docker host, daemon paths are never assumed to exist locally:

```sh
hatch --container remote-dev \
  --config /local/project/.devcontainer/devcontainer.json \
  --host-workspace /local/project --require-config
```

`daemon_host_path` is a bind source on the Docker daemon's machine, not a guaranteed
local folder. Nested volumes shadow parent bind mappings. Herdr integration requires
an accessible project path and a local Unix Docker endpoint; otherwise it falls back
to the current terminal.

## Installation, authentication, and persistence

Hatch first tries `AGENT --version`. If that fails, a detected POSIX shell checks PATH,
`~/.local/bin`, `~/.npm-global/bin`, `~/.opencode/bin`, and `~/.grok/bin`. A present but broken CLI is not overwritten.
`--agent-path /absolute/container/path` selects a custom executable and disables
fallback installation.

Before opening a Herdr workspace or launching locally, Hatch checks the chosen agent
in the resolved container environment. A missing CLI prompts in the current terminal,
defaulting to No. Declining, missing prerequisites, or failed installation creates no
Herdr workspace. Unattended installation
requires explicit opt-in:

```sh
hatch --container my-devcontainer --install --no-input -- --version
```

All native installers run **inside the container as the selected user**. Hatch checks
Linux x86_64/ARM64, required tools, and write permissions. POSIX sh, curl, and common
Unix utilities are needed; every installer except Codex additionally needs Bash.
Agent-specific tools are checked (for example sha512sum for Antigravity). Neither root
nor a package manager is assumed. Native installers may configure container-local shell
files; Hatch does not change host profiles or transfer host credentials. A complete HTTPS download precedes execution, temporary scripts
are cleaned up, and the installed user-local binary is verified before launch. The
option installs missing CLIs; it does not upgrade valid installations. Native agents
may have their own update behavior.

`doctor` checks all seven agent executables and reports authentication status for Codex
and Claude without returning account identifiers or credentials. For the other agents,
authentication is explicitly reported as not checked; use their own login/status commands. It does not install, log in, invoke a model, or weaken
sandbox settings. It uses configured shell probing to match launch behavior. Mount
coverage is reported for absolute executable paths; it does **not** prove that symlink
targets, packages, authentication, or history survive a rebuild. Provision persistence
explicitly in your devcontainer if desired; Hatch does not add volumes or edit config.

Supported agents and official installation sources:

| `--agent` | Executable | Installer |
|---|---|---|
| `codex` (default) | `codex` | [Codex](https://learn.chatgpt.com/docs/codex/cli) |
| `claude` | `claude` | [Claude](https://code.claude.com/docs/en/setup) |
| `copilot` | `copilot` | [GitHub Copilot](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli) |
| `opencode` | `opencode` | [OpenCode 2](https://dev.opencode.ai/v2/docs/) |
| `antigravity` (`agy` alias) | `agy` | [Antigravity](https://antigravity.google/product/antigravity-cli) |
| `cursor` | `cursor-agent` | [Cursor](https://cursor.com/docs/cli/installation) |
| `grok` | `grok` | [Grok Build](https://docs.x.ai/build/overview) |

OpenCode installs V2 when missing and leaves working existing versions intact. Cursor
and Grok use distinct commands instead of their shared `agent` alias. Provider login,
subscriptions, and API keys remain agent-specific and container-local. Hatch installs
only the selected missing agent, not the entire list. Agents may manage their own
background processes and updates; Hatch does not manage those services.

## Automatic Herdr handoff

Inside Herdr (`HERDR_ENV=1`), interactive launches create a **new workspace** with a
fresh terminal, submit an explicit Hatch command, and exit. There is no workspace reuse
registry or destination prompt. Existing panes are untouched and focus is preserved.
Use `--here` to stay in this terminal. Outside Herdr, or if it is unavailable or lacks
a verified local project directory, Hatch launches in the current terminal.

Piped input or redirected output stays local so scripts retain their data and output.
The advanced `--herdr` flag explicitly requests handoff; `--herdr-workspace ID` targets
a specific existing workspace with a fresh tab. Handoff acknowledgement means the
command was queued, not that the agent finished starting.

After successful parent preflight/install, the child receives `--here` to prevent
recursion, `--no-input` to avoid another installation prompt, and the prepared absolute
agent path when available. It rechecks the executable but never installs again. A
container change between preflight and child startup can still make the child fail.
The handoff also includes explicit container, workspace,
user, agent, arguments, config inputs, and Docker connection settings. Non-UTF-8 argv
is rejected before workspace creation; `--here` supports arbitrary Unix bytes. After an
ambiguous pane-run failure, Hatch identifies the created terminal and does not retry
or silently start a duplicate locally. Old Hatch registry files are unused and left intact.

Local launch uses Unix `exec`: the calling shell still waits normally and returns when
the agent exits. Docker receives terminal input, signals and resizing directly, without
a resident Hatch parent needing to forward them. No daemon or polling is introduced.

## Validation and packaging

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
HATCH_TEST_BINARY=target/release/hatch python3 -m unittest discover -s tests -v
python3 scripts/package.py
```

The default tests use disposable Docker/Herdr/resolver executables and fixtures, not
real agents, worktrees, or containers. Native bootstrap tests never download agents.
Opt-in live tests use an existing container and clean their disposable files:

```sh
python3 tests/live_launch.py CONTAINER /usr/bin/python3
python3 tests/live_sandbox.py CONTAINER USER
```

The package script creates a native Linux x86_64/aarch64 archive with stable tar/gzip
metadata, SHA-256, installer, resolver lockfile, docs, and dependency license notices.
Set `CARGO=/path/to/cargo` if necessary. Archives use native GNU/Linux builds, not a
claim of universal/static portability. CI checks and x86_64/ARM64 artifact workflows
are provided; they have not run on GitHub in this workspace. No public release is
published automatically.

See [DESIGN.md](DESIGN.md) for verified behavior, limitations, and the explicit
worktree deferral. Keep using `cf` for its legacy worktree behavior until that later
conversation; the host installer installs Hatch alongside it.
