# Hatch

```text
   _           _       _
  | |__   __ _| |_ ___| |__
  | '_ \ / _` | __/ __| '_ \
  | | | | (_| | || (__| | | |
  |_| |_|\__,_|\__\___|_| |_|
```

Launch coding agents inside devcontainers from your host terminal.

Supports Codex, Claude, GitHub Copilot, OpenCode, Cursor, and Grok Build.

## Quick start

Run Hatch on your **Linux Docker host**. Choose the guided flow or supply the target
and agent directly. Supports Linux `x86_64` (Intel/AMD) and `aarch64` (ARM64).

### Install

```sh
curl -fsSL https://raw.githubusercontent.com/timgabrhel/hatch/main/scripts/install.sh | sh
```

### Choose how to launch

**Interactive:**

```sh
hatch
```

**Direct CLI:**

```sh
hatch --container my-devcontainer --agent claude --no-input --here
```

### Start a container or pass agent arguments

Start or reuse a devcontainer from a project checkout, then launch:

```sh
hatch up --config .devcontainer/devcontainer.json
```

You can also choose **New from devcontainer.json…** in the interactive container menu.
Startup runs the project's normal Dev Container lifecycle hooks.

Forward arguments to an agent after `--`:

```sh
hatch --container my-devcontainer --agent codex --no-input --here -- resume --last
```

Hatch passes these arguments unchanged. See the [CLI reference](docs/cli.md) for
scripting, configuration overrides, and all options.

## CLI overview

| Command | Use it to… |
|---|---|
| `hatch` | Launch through the guided flow. |
| `hatch update` | Update Hatch to the latest release. |
| `hatch up --config PATH` | Start or reuse a devcontainer, then launch. |
| `hatch containers` | Browse running devcontainers interactively. |
| `hatch containers --json` | Get a read-only inventory. |
| `hatch inspect --container NAME` | Check a container's context and available agents. |
| `hatch worktrees --container NAME` | List its Git worktrees. |
| `hatch worktree create PATH --branch NAME --container NAME` | Create a worktree for an existing local branch. |
| `hatch worktree remove PATH --container NAME` | Remove a clean linked worktree, keeping its branch. |
| `hatch options` | See all settings, including advanced options. |

Common options:

| Option | Purpose |
|---|---|
| `--container NAME` | Select a running container. |
| `--agent NAME` | Choose `codex`, `claude`, `copilot`, `opencode`, `cursor`, or `grok`. |
| `--config PATH` | Select a project by its devcontainer config. |
| `--worktree PATH` | Use an existing worktree's absolute path inside the container. |
| `--here` | Stay in this terminal instead of opening Herdr. |
| `--no-input` | Skip Hatch prompts and review; provide an explicit target. |
| `--install` | Allow installation of a missing agent in unattended use. |

For scripts, name the target explicitly:

```sh
hatch --container my-devcontainer --agent codex --no-input --here -- exec 'Run tests'
```

Use `hatch --help` for a short reference or see the [full CLI reference](docs/cli.md).

## Documentation

| Guide | Covers |
|---|---|
| [Installation](docs/installation.md) | Requirements, updates, installer options, native archives. |
| [Guided launch](docs/usage.md) | Config/workspace selection, review, navigation, accessibility. |
| [CLI reference](docs/cli.md) | All commands and options, scripts, JSON, argument forwarding. |
| [Configuration](docs/configuration.md) | Container startup, resolution, environment, remote Docker. |
| [Agents](docs/agents.md) | Supported agents, installation, authentication, prerequisites. |
| [Worktrees](docs/worktrees.md) | Creation, selection, initialization, safe removal. |
| [Herdr](docs/herdr.md) | Workspace destinations, handoff, local fallback. |
| [Development](docs/development.md) | Hatch's devcontainer, tests, packaging. |

[DESIGN.md](DESIGN.md) tracks architecture, implementation status, and limitations.
[AGENTS.md](AGENTS.md) contains contributor instructions.
