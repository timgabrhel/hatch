# Configuration and container startup

[← Back to README](../README.md)

```sh
hatch --config /path/project/.devcontainer/devcontainer.json
hatch --config /path/project/.devcontainer/linux/devcontainer.json
# Explicit unattended start/create, then launch:
hatch up --config /path/project/.devcontainer/devcontainer.json --no-input
```

## Container matching and startup

Matching uses both config and workspace paths. A running match is reused directly,
without rebuilding or rerunning lifecycle hooks.

If no running match exists, an interactive launch offers to start/create one (default Cancel); unattended launches
require `up`. The confirmation offers **Start container** when a container with the
matching config/workspace labels exists, or **Create container** otherwise. Startup
is followed by session selection and review; the startup button does not launch the agent.

The official Dev Container CLI handles stopped containers, image pulls,
Dockerfile builds, Features, and Compose startup. It runs the project's normal lifecycle
hooks, including host `initializeCommand`. Interactive startup shows up to eight lines
of logs in an inline pane. Use Up/Down or k/j to scroll, and f to follow new output.
The pane retains the most recent 2,000 lines; long lines are clipped to its width.
Ctrl-C cancels startup and restores terminal settings. Plain mode and redirected
output stream logs to stderr without the pane.

Every startup saves full stderr and the CLI result to a private (0600)
`hatch-devcontainer-*.log` file in the system temporary directory (`TMPDIR` when set).
The readiness summary includes its absolute path as **Dev container log**, with a
muted field label. Rebuild warnings follow that summary in dim yellow. Log panes use
muted cyan headings and gray text, indented to align with other output, with a blank line
above the pane. Failure messages also include the absolute log path. Cancelling before
startup removes the reserved empty log file.
The log survives Hatch exiting, until removed manually or by temporary-file cleanup.
It can include whatever project hooks print; displayed log lines are sanitized in the pane.

Normal launch and `up` never request container removal or a forced rebuild. Failed startup leaves any created resources intact for diagnosis;
it does not launch an agent or automatically retry.

## Explicit rebuild

Choose **Rebuild a devcontainer…** in the container picker, then select an existing
target (running or stopped), choose the agent, and confirm **Rebuild container**.
Hatch uses the target's config/workspace labels; if they are missing or inaccessible,
supply local source paths through the CLI instead:

```sh
hatch rebuild --config .devcontainer/devcontainer.json
hatch rebuild --config .devcontainer/devcontainer.json --no-cache
# Explicit unattended replacement and launch:
hatch rebuild --config .devcontainer/devcontainer.json --agent codex --no-input
```

Exactly one container must match both identity labels. The default-Cancel confirmation
explains that replacement stops its sessions and discards files outside persistent mounts.
Hatch delegates to `devcontainer up --remove-existing-container --expect-existing-container`,
retaining the build cache unless `--no-cache` adds `--build-no-cache`. Normal lifecycle
hooks run, with the scrolling startup log pane and retained failure logs. A failed rebuild
can leave the old container removed; Hatch never attempts another replacement automatically.
After success, checkout choices and the final session review proceed normally.
Backing out after completion or launching a Herdr child does not repeat the rebuild.

## Workspace and identity

Standard `.devcontainer/devcontainer.json`, `.devcontainer/PROFILE/devcontainer.json`,
and `.devcontainer.json` locations infer the workspace. For other layouts, specify
`--host-workspace /path/project`; remote Docker always requires it. Source paths are
canonicalized.

Profiles with missing identity labels need explicit `--container` to
attach; Hatch does not guess based on names. Multiple running matches prompt or fail
in unattended mode. `inspect` and read-only inventory never start containers.

## Startup behavior and limits

`up` requires `--config` and does not accept `--container`. It uses `--no-lockfile`
to avoid generating a Dev Container lockfile. Hatch does not edit devcontainer.json;
project hooks retain their own normal behavior.

There is no editor extension host or automatic `forwardPorts` tunnel; Docker-published ports and explicit project networking
still apply. Existing running containers attach without `postAttachCommand`.

There are no RAIC-specific names, users, paths, or setup commands in runtime code.
Discovery uses running containers' devcontainer labels, mounts, and configured user.
Ordinary containers work with explicit `--container`, `--workspace`, and `--user`.

## Merged configuration

Once a running container is selected, Hatch resolves configuration with:

```text
devcontainer read-configuration --container-id ID --config FILE \
  --workspace-folder FOLDER --include-merged-configuration
```

The official resolver handles JSONC, substitutions, local config, and image/feature
metadata. Hatch applies the resulting `workspaceFolder`, `remoteUser`, and `remoteEnv`,
and checks Compose service identity against the selected container.

The source config path comes from the container label, or standard config locations under a
verified local workspace. Multiple profiles require a choice or explicit `--config`.

`--workspace` and `--user` override resolved values. `inspect` exposes the actual
config file, local workspace, sources, warnings, environment **names**, and
userEnvProbe policy. Agent availability checks use the launch environment, including
shell probing when configured; environment values are not revealed.

Resolver failures hide raw logs because those can contain credentials. Malformed
available config is an error, not a silent fallback.

## Shell environment

Launch and `inspect` apply `userEnvProbe` (`none`, `loginShell`, `interactiveShell`, or
`loginInteractiveShell`; default `loginInteractiveShell` for resolved configs). Hatch
detects the user's passwd shell and home, probes the environment, then applies
`remoteEnv` overrides. Supported probe shells are sh/bash/dash/ash/zsh/ksh. Shell
startup scripts execute as that container user; project lifecycle hooks do not.

Unsupported/missing probe prerequisites produce an error; use `userEnvProbe: none`
and `remoteEnv` for containers without a supported shell.

Setting `remoteEnv` to `null` uses container `env -u` to remove the variable before exec.

No host authentication or history is automatically copied; config-declared localEnv substitutions and
existing container mounts continue to apply.

## Metadata fallback

When the CLI or source config is unavailable, Hatch reports metadata-only fallback:
workspace from a local-folder label mapped through a bind mount, then non-root Docker
WorkingDir; user from remoteUser metadata, Docker Config.User, then UID 0.

Override image defaults when needed. Use `--require-config` to fail rather than fall back,
or `--metadata-only` to intentionally skip source resolution and shell probing.

## Remote Docker

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
