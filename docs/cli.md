# CLI reference

[← Back to README](../README.md)

```text
hatch [OPTIONS] [COMMAND] [-- AGENT_ARGS...]
```

Use `hatch --help` for everyday options, `hatch COMMAND --help` for a command,
`hatch options` for all settings, and `hatch --version` for the installed version.
Options can be combined with commands. Everything after the first `--` belongs to
the selected agent.

## Commands

| Command | Behavior |
|---|---|
| `hatch` | Guided launch; use explicit inputs for scripts. |
| `hatch update` | Update the host executable in place from the latest GitHub release. |
| `hatch up --config PATH` | Reuse, start, or create a devcontainer, then launch. |
| `hatch containers` | Open the container browser in an interactive terminal. |
| `hatch containers --json` | Return a read-only inventory of running devcontainers. |
| `hatch inspect` | Select one container and inspect its context and available agents. |
| `hatch worktrees` | List worktrees in the selected running container. |
| `hatch worktree create PATH --branch NAME` | Create a checkout of an existing local branch. |
| `hatch worktree remove PATH` | Remove a clean linked worktree, keeping its branch. |
| `hatch options` | Explain all settings. |

`up` requires `--config` and cannot be combined with `--container`. Worktree management
requires an existing running container. See [configuration](configuration.md) and
[worktrees](worktrees.md) for the full behavior.

## Update Hatch

```sh
hatch update
```

Checks the latest release and updates this installation only when newer. Downloads
are verified before atomic replacement. No Docker access, agent installation, or
launch occurs; container/agent options and arguments after `--` are rejected.
The installation directory must be writable. See [installation](installation.md).

## Interactive or direct launch

```sh
# Guided selection and final review:
hatch
# Preselect the container and agent, keeping checkout selection and review:
hatch --container my-devcontainer --agent claude
# Skip Hatch prompts and launch in the current terminal:
hatch --container my-devcontainer --agent claude --no-input --here
```

Explicit options skip the corresponding selection steps. `--no-input` skips all
Hatch prompts and the final review; it does not make the agent noninteractive.
Add `--install` to authorize installation of a missing agent in this mode.
The session summary's **CLI equivalent** command shows the public CLI options for your
selections, including `--no-input` to bypass the guided flow next time.

## Command-specific options

| Command | Option | Purpose |
|---|---|---|
| `containers`, `inspect`, `worktrees` | `--json` | Print machine-readable output. |
| `worktree create PATH` | `--branch NAME` | Required local branch name. |
| `worktree create PATH` | `--from REF` | Create a new branch from an existing local ref or commit; no fetch. Without it, the branch must already exist. |
| `worktree remove PATH` | `--yes` | Confirm removal and acknowledge no session is using the checkout. Required when removal cannot prompt. |

Worktree paths must be absolute paths inside the container. See the
[worktree guide](worktrees.md) for storage requirements and removal checks.

## Target and configuration

| Option | Purpose |
|---|---|
| `--container NAME_OR_ID` | Select a running container explicitly. |
| `--config PATH` | Use a local devcontainer config; selects its container unless `--container` is also supplied. |
| `--host-workspace PATH` | Local project folder or workspace file; requires `--config`. |
| `--workspace PATH` | Override the absolute working directory inside the container. |
| `--user USER` | Override the container user, as a name or UID with optional group. |
| `--metadata-only` | Skip source config resolution and shell environment probing; conflicts with `--config`. |
| `--require-config` | Fail if merged configuration is unavailable; conflicts with `--metadata-only`. |
| `--devcontainer-cli PATH` | Select the resolver executable; defaults to `devcontainer`. |

## Agents and initialization

| Option | Purpose |
|---|---|
| `--agent NAME` | Choose `codex`, `claude`, `copilot`, `opencode`, `cursor`, or `grok`. Defaults to `codex`. |
| `--agent-path PATH` | Use an absolute executable path inside the container; disables fallback installation. |
| `--install` | Allow installation of the selected missing agent; required for unattended installation. Conflicts with `--agent-path`. |
| `--worktree PATH` | Select a registered Git worktree by absolute container path. |
| `--setup-command COMMAND` | Run a project shell command once before agent launch. |
| `--init-skill PATH` | Ask standard Codex or Claude to follow an initialization skill; conflicts with setup commands and agent arguments. |

See [agent installation](agents.md) and [worktree initialization](worktrees.md#initialization).

## Terminal and destination

| Option | Purpose |
|---|---|
| `--here` | Explicit current-terminal launch (the default). |
| `--herdr` | Explicit legacy workspace handoff, or a new Core session attached here when interactive. |
| `--herdr-workspace ID` | Open a fresh tab in a specific legacy Herdr workspace; unavailable in Core. |
| `--no-input` | Skip Hatch prompts; require an explicit target for unattended launch. |
| `--no-tty` | Disable Docker's terminal allocation. |
| `--plain` | Use numbered prompts instead of arrow-key menus. |

Normal launch always uses the current terminal, even inside Herdr/Core; no destination
menu appears. `--here` conflicts with explicit Herdr destination options. Explicit Core
handoffs attach immediately when stdin/stdout are terminals and --no-tty is absent;
otherwise Hatch prints the detached session's attach command. See [Herdr handoff](herdr.md).

## Forward agent arguments

```sh
hatch --container my-devcontainer -- resume --last
hatch --container my-devcontainer -- 'Explain this repository'
hatch --container my-devcontainer -- login --device-auth
hatch --container my-devcontainer --agent claude -- -p 'Explain this repository'
```

Hatch adds no agent flags, resumes nothing implicitly, and does not translate flags
between agents. Choose arguments supported by the agent you selected.

Direct launch preserves separate OS-string arguments, including empty arguments,
shell metacharacters, and non-UTF-8 bytes. It also preserves stdin and agent exit status.

Docker receives `-i` for piped input and `-t` only when stdin/stdout are terminals and
`--no-tty` is absent. `HERDR_AGENT` identifies the host Docker process. Herdr handoffs
quote arguments once and reject non-UTF-8 arguments before creating a workspace.

## Unattended launch

```sh
hatch --container my-devcontainer --agent codex --no-input --here -- exec 'Run tests'
hatch --container my-devcontainer --install --no-input --here -- --version
hatch up --config .devcontainer/devcontainer.json --no-input --here
```

Scripts must name a target even when only one container is running. `--no-input`
skips Hatch's final review but does not silence the agent's own prompts.

## Inventory and inspection

```sh
hatch containers --json
hatch inspect --container my-devcontainer
hatch inspect --container my-devcontainer --json
```

| Command | Scope | JSON shape |
|---|---|---|
| `containers --json` | Running devcontainers found through Docker labels; no executable probes. | Array |
| `inspect --json` | One container's resolved context and all supported agents' availability. | Object |
| `worktrees --json` | Registered checkouts in one running container. | Array |

Ordinary Docker service containers are excluded from devcontainer discovery unless
selected explicitly. Use `docker ps` for Docker's complete running-container list.

Interactive `inspect` asks for a container even when only one is found. Pass
`--container` or `--config` to skip selection; unattended inspection requires one.
`containers` becomes read-only with `--json`, `--no-input`, or nonterminal use.
Neither read-only inventory nor inspection starts containers.

Inspection checks executable paths and versions as the selected development user.
Checks run concurrently, and terminal results stream as they finish. Ctrl-C cancels
the scan. JSON waits for all checks and preserves supported-agent ordering.

Terminal output uses available/missing symbols; plain mode uses `[ok]` and `[x]`.
Inspection does not check authentication or credential mount coverage. Environment
names are reported, but values and raw resolver failure logs are hidden.

The old `doctor`, `agents`, and `menu` commands are not available. Use `inspect`
and bare `hatch` instead.
