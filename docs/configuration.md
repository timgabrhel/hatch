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
require `up`.

The official Dev Container CLI handles stopped containers, image pulls,
Dockerfile builds, Features, and Compose startup. It runs the project's normal lifecycle
hooks, including host `initializeCommand`. Build/hook output streams to stderr and may
include whatever the project's scripts print.

Hatch never requests container removal or a forced rebuild. Failed startup leaves any created resources intact for diagnosis;
it does not launch an agent or automatically retry.

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
