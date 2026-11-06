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

Hatch can select, create, list, and safely remove Git worktrees inside a selected
container. It does not edit your devcontainer configuration, force rebuilds, or replace
the installed `cf`. Explicit startup runs project lifecycle hooks.

## Start here

```sh
hatch                      # Guided container → workspace (for config) → agent → checkout flow
hatch --agent claude
hatch --here               # Stay in this terminal, even in Herdr
hatch options              # Advanced settings explained
hatch inspect              # Select a container; inspect context and agent availability
hatch containers           # Browse interactively, including a pasted config path
hatch containers --json    # Read-only inventory
```

Running `hatch` interactively always asks which container to use, even when only one
is running. It then asks for an agent. `--container` or `--config` supplies the target;
`--agent` skips the agent question. A custom `--agent-path` also skips agent selection.

```sh
# Skip the menus when you know what you want:
hatch --container my-devcontainer --agent claude
# Scripts must identify their target; no implicit single-container launch:
hatch --container my-devcontainer --agent codex --no-input -- exec 'Run tests'
```

The container list includes **New from devcontainer.json…**, even with no running containers.
This flow reuses a matching container when available; creation still requires consent.
Hatch first offers configs detected in your current directory: `devcontainer.json`,
`.devcontainer.json`, `.devcontainer/devcontainer.json`, and
`.devcontainer/PROFILE/devcontainer.json`. Select one or paste another path, then
choose its workspace. If no configs are found, path entry opens directly.
The config flow then asks which agent to run **before** starting or building a
container. Explicit `--agent` or `--agent-path` skips that question. Canceling at the
agent picker does not start a container.
The **Select a workspace** menu lists detected VS Code `.code-workspace` files from
your current directory, beside the config, and in the inferred project root, without
duplicates. Each file shows its relative path including the `.code-workspace` extension.
If no workspace files exist, the menu says so. It also offers a detected project
folder for standard config layouts. You can always paste a workspace file or folder.
Existing workspace files show only their relative path. An empty file uses the detected
project folder and stays empty; if the config has no inferred project root, Hatch asks
for a folder. When no file exists and the project root is known, **Create NAME.code-workspace** explicitly creates a file with `folders: []` in that root.
Choosing a folder alone creates no file. Existing files are never overwritten.
Workspace files support JSON comments, trailing commas, and relative folder paths. A
multi-folder file prompts for the project folder used by the Dev Container CLI. It does
not run editor tasks or open all folders. Remote URI entries and variable-based folder
paths require you to choose a concrete local folder instead. Missing/invalid paths are
reported so you can correct them; Escape returns immediately to the previous menu,
even with a partly entered path. Empty Enter or `q` also goes back. `~/` and quoted pasted
paths work without shell evaluation.

Local config/workspace paths are displayed relative to the directory where you run
Hatch (`.` for that directory, `../` for paths outside it). Config and workspace
selections are separated by headings and blank lines. JSON and execution arguments
retain absolute paths; paths inside containers remain absolute.
Action summaries use colored headings and compact, aligned labels. The startup
summary includes the selected agent, config, and workspace before confirmation.
Plain/no-color modes remain supported.

`--host-workspace` also accepts a folder or a single-folder `.code-workspace` file;
for a multi-folder file unattended, pass the intended folder explicitly. Interactive
`hatch containers` opens this same browser. `--json`, `--no-input`, or nonterminal use
keeps it a read-only inventory.

Menus use arrow keys, Enter and Esc/Back, with color, paging and terminal restoration.
The agent and config lists omit “Back to containers”; use Escape (or q in numbered mode).
The agent menu shows all agents together when terminal height permits; the five-item
page limit applies only to choices with path detail lines.
Each interactive flow starts with a short heading and description: **Launch agent in
container** for launching or **Inspect container** for diagnostics. These headings
appear once, also in plain mode, and are omitted for JSON.
Navigation guidance appears once. The session summary identifies container, workspace,
user, agent and Herdr destination after the final destination choice.
It also lists pending installation, setup/initialization and agent arguments.
Missing agents show a yellow **! Install** status and a single **Install Agent & Launch**
action. Available agents show **Installed** and use **Confirm and launch**. There is no separate install prompt. Container selection is shown in the summary without repeated selection notices.
Interactive launch pauses here: the final action must be confirmed before agent
installation, setup commands, terminal creation or launch. Back/Escape revisits
the previous choice; Back is the default. The arrow-key review clears when leaving,
so returning redraws one summary. Numbered mode reuses an unchanged summary. `--no-input` skips this interactive review. The stylized lowercase wordmark falls
back to HATCH on narrow terminals, with the build version below the tagline.
Container, workspace and checkout choices show a name with a separate indented `Path:`
line. Escape returns to the previous menu, including in numbered mode; at the first
menu it exits. Checkout menus use Back instead of Cancel launch.
Notices and errors use short blocks and wrapped text.
`--plain`, `TERM=dumb`, or `HATCH_PLAIN=1` select numbered questions; `NO_COLOR` disables
color. No full-screen interface is used. `inspect` shows a readable terminal summary;
`--json` or piped output provides complete machine-readable details.

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
  is an **object**. It always prompts for a container in a terminal, even with one
  candidate, unless you supply `--container` or `--config`. Unattended inspection
  requires one of those explicit targets.
- Bare `hatch` always shows the container picker in an interactive terminal, even
  with one candidate. It never hardcodes RAIC. For scripts, supply the target explicitly.

`hatch inspect` also checks availability in the selected container as its development user:
✓ available, ✗ missing or unusable. It asks which container even when only one is running.
Use `hatch inspect --container NAME` or `--config PATH` to skip selection. Unattended
use requires one of those explicit targets. `--json` reports the target and availability.
The list contains all supported agents with their actual availability, not just installed agents.
Checks run concurrently (one worker per supported agent). The terminal shows results
as they finish, so a slow check does not delay other rows; Ctrl-C cancels the scan.
JSON is emitted once all checks finish and keeps the supported-agent ordering.
`--plain` uses `[ok]` and `[x]`; `NO_COLOR` removes color without hiding status symbols.
`hatch inspect` includes executable paths in terminal and JSON output. It performs
read-only locate/version probes using the launch environment. Authentication and mount
coverage are not checked or reported. The redundant `doctor`, `agents`, and `menu`
commands have been removed; bare `hatch` provides the guided launch flow.

## Develop Hatch in its own devcontainer

`hatch.code-workspace` is an empty VS Code workspace placeholder (`folders: []`).
Selecting it in Hatch uses the detected project folder without editing the file.

The repository's `.devcontainer/devcontainer.json` uses Microsoft's Rust Bookworm
image and Node.js 22. Its setup hook ensures rustfmt
and clippy, fetches locked Cargo dependencies, and installs the pinned Dev Container
CLI under `tooling/node_modules`. The workspace is `/workspaces/hatch`, owned by the
checkout's bind mount, with `vscode` as the development user.
Its `runArgs` sets `--name=hatch-dev` for newly created containers. Existing containers
are not renamed automatically.

Open the folder with VS Code's **Reopen in Container**, or from the Docker host:

```sh
hatch up --config .devcontainer/devcontainer.json --host-workspace . --agent codex
```

This project's config explicitly shares the host's file-based Codex, Claude, and GitHub
authentication. It mounts `~/.codex/auth.json`, `~/.claude/.credentials.json`, and
`~/.config/gh/hosts.yml` read-only directly at their usual paths under `/home/vscode`.
All three host files must exist and be readable by the development user's
mapped UID. GitHub CLI is installed and configured as Git's HTTPS credential helper.
The creation hook gives `vscode` ownership of the mount parent directories, which
Docker may create as root. It does not change the mounted credential files.
No credentials are copied. Agents cannot save refreshed tokens to these read-only
files. If a host login replaces a credential file, recreate the container to remount
the new file. Host files are never written; histories and other host settings are not
mounted. Keychain-only logins
and SSH authentication are not forwarded by this config.

These mounts take effect when creating/recreating the devcontainer, not when attaching
to an existing one. This is an explicit project configuration, not a Hatch-wide default.
There are no agent installations or Docker socket mounts
in this config. Agents can be installed by Hatch's normal prompt after startup. Default
Rust tests use fixtures and need no Docker daemon. For live discovery, run Hatch
from the Docker host. Dependencies require network access on initial setup. The image
and Feature major tags track supported updates rather than pinning immutable digests.

Inside the development container:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

See [AGENTS.md](AGENTS.md) for contributor/agent instructions and test boundaries.

## Build and install

Use **current stable Rust** and Docker on the host. Tests require Git and standard Linux shell
tools, with no separate language runtime. The optional
full configuration resolver requires host Node.js/npm; neither is needed for the Rust
launcher itself or metadata-only mode.

```sh
cargo build --release --locked
scripts/install.sh --with-resolver
# Installs ~/.local/bin/hatch and the pinned official Dev Container CLI.
# No shell profiles, cf, devcontainer config, or container agents are changed.
```

If your PATH still selects an old distro Cargo, use `source "$HOME/.cargo/env"` first.
The installer runs the newly installed Hatch immediately; use `--no-launch` for
installation only (including unattended installation). Run the installer on the host;
devcontainer builds are available through the shared checkout's `target/release/hatch`.
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
hatch --config /path/project/.devcontainer/linux/devcontainer.json
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
in unattended mode. `inspect` and read-only inventory never start containers.

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

Launch and `inspect` apply userEnvProbe (`none`, loginShell, interactiveShell, or
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

Hatch does not probe credential persistence or edit your devcontainer configuration.

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
Agent-specific tools are checked (for example readlink for Grok). Neither root
nor a package manager is assumed. Native installers may configure container-local shell
files; Hatch does not change host profiles or transfer host credentials. A complete HTTPS download precedes execution, temporary scripts
are cleaned up, and the installed user-local binary is verified before launch. The
option installs missing CLIs; it does not upgrade valid installations. Native agents
may have their own update behavior.

`inspect` checks all six agent executables and includes availability and executable paths
under `agents` alongside its existing launch-context JSON fields. Authentication
checks are absent. Use the agent itself to log in or verify access.
These diagnostics do not install, log in, invoke a model, or weaken
sandbox settings. It uses configured shell probing to match launch behavior. Provision persistence
explicitly in your devcontainer if desired; Hatch does not add volumes or edit config.

Supported agents and official installation sources:

| `--agent` | Executable | Installer |
|---|---|---|
| `codex` (default) | `codex` | [Codex](https://learn.chatgpt.com/docs/codex/cli) |
| `claude` | `claude` | [Claude](https://code.claude.com/docs/en/setup) |
| `copilot` | `copilot` | [GitHub Copilot](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli) |
| `opencode` | `opencode` | [OpenCode 2](https://dev.opencode.ai/v2/docs/) |
| `cursor` | `cursor-agent` | [Cursor](https://cursor.com/docs/cli/installation) |
| `grok` | `grok` | [Grok Build](https://docs.x.ai/build/overview) |

OpenCode installs V2 when missing and leaves working existing versions intact. Cursor
and Grok use distinct commands instead of their shared `agent` alias. Provider login,
subscriptions, and API keys remain agent-specific and container-local. Hatch installs
only the selected missing agent, not the entire list. Agents may manage their own
background processes and updates; Hatch does not manage those services.

## Git worktrees

For a Git repository, interactive launch adds a **checkout / worktree** choice after
container and agent selection: keep the current checkout, select an existing checkout,
create one, or remove one. This is separate from selecting a folder in a VS Code
`.code-workspace` file. Explicit `--workspace` or `--worktree` skips the checkout picker;
unattended launches keep their existing working directory unless `--worktree` is supplied.

```sh
hatch worktrees --container my-devcontainer --json
hatch --container my-devcontainer --worktree /workspaces/project.worktrees/topic
# Attach an existing local branch:
hatch worktree create /workspaces/project.worktrees/topic --branch topic --container my-devcontainer
# Create a new branch from an explicit existing ref (no fetch):
hatch worktree create /workspaces/project.worktrees/topic --branch topic --from origin/main --container my-devcontainer
hatch worktree remove /workspaces/project.worktrees/topic --container my-devcontainer --yes
```

All paths above are **container paths**. Git runs inside the selected container as its
intended development user; host Git is not used to mutate the repository. Management
requires an existing running container and never starts one, installs agents, fetches,
or creates terminals. Unattended management requires `--container` or `--config`.
`worktrees --json` returns an array with `path`, `head`, `branch`, `bare`, `locked`, and
`prunable` fields. Paths are absolute; `head`/`branch` can be null.

Creation requires an existing bind mount or named volume covering both the checkout
and its shared Git metadata. Reusing those mounts on future rebuilds remains the
project's responsibility; current coverage alone cannot prove future persistence.
Nested destination directories are supported; an existing
destination is never reused or overwritten. Host-shared mounts must preserve the same
relative layout on the host and in the container. Relative Git links require Git 2.48+
in both environments; creating them may enable Git's `relativeWorktrees` repository
extension, making the repository incompatible with older Git. Hatch checks local Git
when the mapped source paths are actually accessible; remote host Git compatibility is
the user's responsibility. With older Git, absolute links are allowed only for identical
host/container paths or container-only volumes. Hatch checks existing checkout links
inside the container before selecting them, but does not repair existing links or edit
mounts. Normal Git hooks may run when creating a checkout.

After selecting or creating a worktree, initialization is optional and defaults to
launching without it. Choose an explicit project shell command or ask Codex/Claude to
read an initialization skill. The skill picker lists `.agents/skills/*/SKILL.md` in the
main and selected checkout, preferring the main clone; another absolute container path
can be entered. Skills are offered by path, not automatically executed or interpreted
by Hatch. For scripts:

```sh
hatch --container my-devcontainer --worktree /workspaces/project.worktrees/topic \
  --setup-command './scripts/init-worktree.sh'
hatch --container my-devcontainer --worktree /workspaces/project.worktrees/topic \
  --init-skill /workspaces/project/.agents/skills/project-init/SKILL.md
```

`--setup-command` explicitly runs POSIX `sh -c` once inside the selected directory, with
stdin disconnected, after agent preparation and before any Herdr terminal is created.
Failure stops launch and retains the checkout. `--init-skill` instead supplies one initial
natural-language prompt asking the agent to follow the skill's existing-worktree init
workflow. It supports the standard Codex and Claude executables, conflicts with arguments
after `--` and `--setup-command`, and verifies that the skill file is readable. Other
agents can receive a user-written initialization prompt after `--`. A prompt handoff is
not proof initialization succeeded; the agent reports its result. No submodule update,
dependency installation, credential copying, or editor opening is added automatically.

Removal requires an explicit confirmation, or `--yes` acknowledging that no session is
using the checkout. It refuses the main/current checkout, locked/unavailable worktrees,
and tracked, untracked, ignored, or submodule changes. Git may also refuse populated
submodules. Hatch never forces removal, deletes branches, prunes, or cleans up after an
agent exits. It cannot reliably detect every active session. Remove files or resolve
submodule state yourself when needed; failed creation/setup leaves resources intact.
Worktrees share repository history and the selected container; they are not isolated
application stacks. Selection changes the agent's working directory; the selected
container configuration and its environment still apply. `cf` and project-specific
worktree scripts remain unchanged.

## Automatic Herdr handoff

Inside Herdr (`HERDR_ENV=1`), interactive launches ask whether to open a **new workspace**
or **this workspace (new tab)**. This is the last destination choice before the session review, after container startup,
checkout/initialization selection. The review summary appears next, with install
an **Install Agent & Launch** action if the agent is missing. Actual agent
installation starts only after **Install Agent & Launch**; backing out does not install.
The current workspace comes from
`HERDR_WORKSPACE_ID`, never another client's focused workspace. Hatch creates a fresh
terminal, submits an explicit Hatch command, and exits. Existing panes are untouched
and focus is preserved. No workspace reuse registry is used.
New Herdr workspaces are named after the selected container, without a prefix or agent name.
Use `--here` to stay in this terminal. Outside Herdr, or if it is unavailable or lacks
a verified local project directory, Hatch launches in the current terminal.

Piped input or redirected output stays local so scripts retain their data and output.
Explicit `--here`, `--herdr`, and `--herdr-workspace ID` bypass the destination question.
The advanced `--herdr` flag requests a new workspace; `--herdr-workspace ID` targets
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
HATCH_TEST_BINARY=target/release/hatch cargo test --locked --tests
cargo run --locked --example package
```

`cargo test` runs unit tests, process integration tests, and real PTY menu tests.
The Rust `hatch-test-fixture` binary doubles as Docker, Herdr, the resolver, and curl
inside disposable test directories; it is not shipped in release archives. Tests
isolate HOME and PATH and never contact real agents, worktrees, or containers.
Native bootstrap tests never download agents.

Live tests are ignored by default. Opt in to each separately with an existing container
and intended development user. The terminal test requires `/usr/bin/env`, `/bin/sh`,
`id`, and `stty` inside it. The sandbox test requires an already installed Codex and
standard shell tools, writes only in its own temporary directory, and cleans it up:

```sh
HATCH_LIVE_CONTAINER=CONTAINER HATCH_LIVE_USER=USER cargo test --locked --test live terminal_launch -- --ignored
HATCH_LIVE_CONTAINER=CONTAINER HATCH_LIVE_USER=USER cargo test --locked --test live sandbox_read_only -- --ignored
```

The Rust packaging example uses GNU tar, gzip, and sha256sum to create a native Linux
x86_64/aarch64 archive with stable tar/gzip
metadata, SHA-256, installer, resolver lockfile, docs, and dependency license notices.
Set `CARGO=/path/to/cargo` if necessary. Archives use native GNU/Linux builds, not a
claim of universal/static portability. CI checks and x86_64/ARM64 artifact workflows
are provided; they have not run on GitHub in this workspace. No public release is
published automatically.

See [DESIGN.md](DESIGN.md) for verified behavior and limitations. The host installer
installs Hatch alongside `cf`; it does not replace or modify that wrapper.
