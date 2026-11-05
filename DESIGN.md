# Hatch design and implementation status

Updated 2026-11-05. Version 0.5.1. Worktree behavior is **on hold at the user's request**;
there is no worktree implementation, and RAIC's special initialization is not generalized.

## Goals and boundaries

- Find or start/create devcontainers from config, then launch agents; Codex default, six other agents selectable.
- Main-workspace launch needs no worktree. Resume is explicit agent argument forwarding.
- Dynamic discovery across projects, images, Compose services, mounts, and remote Docker.
- Polished inline terminal interaction, explicit unattended commands, no full-screen UI.
- Small synchronous Rust implementation. Current-terminal launch replaces Hatch with
  Docker using Unix exec. No Hatch daemon, watch loop, or resident launcher.
- Optional Herdr handoff creates a fresh terminal, passes explicit inputs, and exits.
- Install agents only inside the selected container as the selected development user.
  Check tools and permissions; never assume Bash, package management, or root.
- No automatic host credential/history transfer, devcontainer edits, forced rebuilds,
  worktree mutations, or installed cf replacement. Explicit startup runs project hooks.

## Implemented surface

| Command/option | Behavior |
|---|---|
| `hatch` | Guided container/agent selection in terminals; explicit target required in scripts |
| `up --config PATH` | Reuse a running match, otherwise official CLI start/create, then launch |
| `menu` | Container or pasted config → workspace when needed → agent, defaults/Back/Esc |
| `containers [--json]` | Interactive browser; JSON/nonterminal/--no-input is read-only inventory |
| `inspect [--json]` | Readable terminal summary or JSON provenance, env names only |
| `doctor [--json]` | Readable terminal status or full JSON including mount coverage |
| `--agent`, `--agent-path` | Agent choice or explicit in-container executable |
| `--container`, `--workspace`, `--user` | Explicit target overrides |
| `--config`, `--host-workspace` | Explicit local source config for resolver/remote Docker |
| `--devcontainer-cli`, `--require-config`, `--metadata-only` | Resolver selection and fallback policy |
| `--install` | Install only if missing; unattended opt-in; no automatic upgrade |
| `--no-input`, `--no-tty`, `--plain` | Prompt, terminal allocation, accessibility controls |
| `--here` | Disable automatic Herdr handoff |
| `--herdr`, `--herdr-workspace` | Advanced explicit handoff/target overrides |
| `options`, `agents` | Full settings explained; list supported agents |
| `-- ...` | Exact agent argv; shell interpretation only at quoted Herdr boundary |

## Configuration and execution

`provision.rs` selects by canonical config/workspace labels before resolution when
--config is supplied without --container. Standard layouts infer the local workspace;
nonstandard and remote Docker layouts require --host-workspace. A running match attaches
without hooks. No match offers a default-Cancel startup prompt, or requires explicit
`up` unattended. Read-only commands never start containers; the interactive containers browser is a launch flow. Startup delegates image,
Dockerfile, Feature and Compose handling to `devcontainer up`, with both identity labels
explicit, no forced removal/rebuild, and --no-lockfile. Normal project lifecycle hooks
run, including host initializeCommand; stderr streams their build/progress output.
stdin remains reserved for the agent. A successful JSON result must identify a running
container with the requested labels before launch. Failure leaves resources intact.
Hatch creates no editor host or automatic forwardPorts tunnels.

`discovery.rs` inspects Docker labels, mounts, and configured user. It contains no
RAIC-specific container names, usernames, paths, or setup commands. Component-aware
mount mapping respects parent binds and nested volume shadowing.

`configuration.rs` delegates JSONC/substitution/metadata merging to the official Dev
Container CLI, pinned to 0.90.0 in optional host tooling. The resolver invokes only
`read-configuration` with the selected running container ID. It never uses up/build,
executes project hooks, or modifies source configuration. Config comes from an explicit
path, the container's config label, or standard local workspace locations. Multiple
profiles prompt or require --config. Compose service identity must match the selected
container. Source config failures are explicit; absent tooling/config is a reported
metadata fallback unless --require-config is set. --metadata-only is an explicit bypass.

Workspace and user overrides win over resolved workspaceFolder/remoteUser. Resolved
remoteEnv overrides the probed user environment; null values use container env -u.
Only variable names are serialized by inspect/doctor. Raw resolver/probe error output
is withheld because it can contain secrets. Config-declared localEnv substitutions
remain supported; there is no additional host credential synchronization.

Launch and doctor honor userEnvProbe, defaulting to loginInteractiveShell for merged
configs. The selected UID's home/shell are detected through passwd; supported probe
shells are sh/bash/dash/ash/zsh/ksh. Shell startup runs as that user, then remoteEnv is
applied. Inspect and container inventory never run shell profiles. Containers without
these prerequisites can use userEnvProbe:none and explicit remoteEnv. Unsupported
shells fail visibly rather than silently selecting Bash.

`launch.rs` retains OS-string argument boundaries and stdin, chooses Docker TTY only
for terminal streams, sets host HERDR_AGENT, and execs Docker. Optional env -u inside
the container also execs its target, leaving no shell wrapper.

## Agent lifecycle

`install.rs` distinguishes missing executables from broken existing installs. Direct
probe succeeds without any shell; fallback detection uses POSIX sh and standard
user-local bin locations. Installation is confirmed interactively or explicitly
requested with --install. Official native installers are fully downloaded before
execution; partial downloads never run. Linux architecture, tools and write access
are checked. All installers except Codex require Bash; none assumes root or a package manager.
Post-install --version must succeed. Existing configuration/mounts remain untouched.
Environment preparation and ensure/install occur in the parent before any Herdr call.
Decline/failure creates no terminal. The child receives --no-input, no --install, and
an absolute executable override when available; its launch still revalidates availability.
A race where the container changes after handoff remains an explicit limitation.

`doctor.rs` detects seven agents and redacts Codex/Claude auth-status output. Other agents
report authentication as not checked, without probing undocumented status commands. It uses launch shell
probing but does not install, log in, invoke a model, or weaken sandboxing. Executable
mount coverage is only a persistence hint, not a guarantee for package symlinks,
authentication, or history. Persistence is an explicit devcontainer decision.

## Menu UX

Interactive default launch always selects a container, including a singleton. CLI target
options bypass that step; explicit --agent bypasses the agent step except in forced menu
mode. A custom --agent-path skips agent choice. Unattended default launches require a
container/config target, even with one running container. `agents` shows two labeled
columns (display name and a direct launch example), not internal executable identifiers.

The container browser offers a pasted devcontainer.json. `workspace_input.rs` searches
same-directory and inferred-root .code-workspace files, offers a standard inferred folder,
and accepts manual file/folder input. JSONC comments/trailing commas are normalized without
altering quoted strings, then serde_json validates syntax. Relative folder paths resolve
against the workspace file, multiple roots require selection, unsupported URI/variable
paths are explicit errors. No editor tasks/settings are executed. Flags accept a workspace
file too; multi-root unattended usage requires an explicit chosen directory.

Dialoguer/console keep inline arrow-key menus, paging, Back/Esc and terminal restoration.
Navigation help appears once. A stylized lowercase wordmark becomes HATCH on narrow
terminals. Path prompts accept blank input to go back, quotes and ~/, without shell
execution. A session card identifies container/workspace/user/agent/destination. Notices
and errors wrap into short blocks; inspect/doctor render readable terminal summaries,
with --json or redirected output preserving full JSON. Plain and NO_COLOR modes remain.

## Herdr

`herdr.rs` is automatic when HERDR_ENV=1 and stdin/stdout are terminals. --here
opts out. Piped data and redirected output remain local; advanced --herdr explicitly
requests handoff. Unavailable Herdr or missing local path mapping falls back locally.
Every default handoff creates a fresh workspace, with no registry or reuse. Explicit
--herdr-workspace validates the target and creates a new tab. Focus is preserved.
The old registry is unused and left intact; fs2 and sha2 dependencies were removed.

The child receives an absolute Hatch path, --here (preventing recursion), explicit
selected inputs, canonical config/workspace, and pinned Docker connection variables.
Agent selection keys differ from executable names for Cursor and Antigravity, and
handoff preserves that distinction. Arguments are shell-quoted once. Non-UTF-8 argv
fails before mutations; direct launches retain it. Empty pane-run success is accepted.
Ambiguous errors identify the new terminal without retrying or duplicating locally.
Local launch continues to exec Docker for terminal/signal/exit handling without a parent.

## Packaging and migration

The per-user installer atomically installs Hatch alongside cf and optionally installs
the pinned official resolver with npm. It never replaces an existing devcontainer
command, edits shell profiles, or installs container agents. A native Linux archive
includes binary, docs, installer, resolver lockfile, checksums, and dependency notices.
Archive metadata is deterministic. Native builds are dynamically linked, not guaranteed
portable across all Linux distributions. CI checks and native x86_64/ARM64 artifact
workflows are included; external CI/public publishing has not run here.

cf remains unchanged, including its project-specific worktree behavior. Replacement
and compatibility shims are deferred with the worktree discussion.

## Validation and practical limits

Automated tests cover exact argv/non-UTF-8 forwarding, stdin/exit/PID behavior, TTY and
menu navigation/restoration, discovery layouts/ambiguity, installation consent/failures,
partial-download cleanup, merged configuration/overrides/redaction, Compose mismatch,
null remoteEnv, source profile discovery, doctor privacy, Herdr quoting/new-workspace/fallback,
and avoiding duplicate launches after handoff failure. Formatting and strict clippy pass.

Historical live checks (0.2/0.3, before subsequent container changes) verified:

- Correct user/workspace, piped input, exit status, Docker process replacement, resize,
  terminal input, and Ctrl-C return to the host.
- Official JSONC resolution, substitutions, remoteEnv values/removal, and no lifecycle hooks.
- Both installed CLI versions launch. Claude authentication and a no-tools, nonpersistent
  model response succeeded. Codex authentication is not yet configured; login is a user action.
- Codex's native sandbox executes commands and permits reads while denying a disposable
  write under the current read-only profile. No sandbox settings were weakened.
- Herdr creates one isolated workspace, reuses it with a second fresh terminal, forwards
  commands correctly, and leaves no Hatch wrapper. Only test-created workspaces were closed.
- Both executable paths currently lack persistent mount coverage. Rebuild persistence
  is not claimed; no rebuild or automatic volume/config change was performed.

No agents were installed merely to run tests. Native installer control flow is exercised
with disposable fixtures; real agents already installed by the user were used for
acceptance. Tests do not create or remove real worktrees. ARM64 artifacts/CI await
execution on their target runner; remote Docker behavior has fixture coverage, not a
live remote-daemon acceptance run. Long-running external startup scripts can delay
launch; Hatch does not start a polling service or daemon to monitor them.

## Deferred worktrees

Do not implement branch/worktree menus or lifecycle until the user returns to this
conversation. RAIC's current helper additionally initializes submodules, copies a
project-specific .npmrc, syncs dependencies, and launches an editor. Those are not
Hatch defaults. Future discussion must separate generic Git worktrees from those
project-specific steps, persistence/link topology, and active-session removal safety.

## References

- [Dev Container specification](https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md)
- [Official resolver implementation](https://github.com/devcontainers/cli/blob/main/src/spec-node/devContainersSpecCLI.ts)
- [Docker exec](https://docs.docker.com/reference/cli/docker/container/exec/)
- [Rust Unix exec](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.exec)
- Herdr's installed `--help` and `--skill` are the authority for the installed API.

## Worktree findings and proposed ownership (not implemented)

RAIC's `.devcontainer/scripts/worktree.sh` combines generic Git operations with:

- Fixed sibling paths: `/workspaces/raic-v2` and `/workspaces/raic-v2.worktrees`,
  backed by host sibling bind mounts. Both persist across container rebuilds.
- Relative Git links (`worktree.useRelativePaths`, Git 2.48+ on both sides) so host
  and container absolute paths can differ while the relative layout stays the same.
- Branch selection from local/origin refs, otherwise a new branch from origin/main;
  cf fetches first, while the helper itself uses already-fetched refs.
- Submodule initialization, copying the ignored UI .npmrc (contains a personal token),
  and `UV_LINK_MODE=copy uv sync` in apps/raic-fed/apps. Copy mode avoids hardlinks
  across the uv-cache volume and worktree bind mount.
- Opening the RAIC code-workspace in VS Code. cf adds Herdr orchestration and starts
  Claude in the tooling subdirectory with the checkout root passed via --add-dir.
- Cleanliness checks, forced removal to accommodate initialized submodules, pruning
  empty parent directories, and retaining the branch (cf separately offers deletion).

Host-side Git worktree creation/listing/removal is feasible. The current host Git is
2.55.0. It requires compatible Git in both environments, persistent mounts with matching
relative topology, and working submodule links; moving only the command to the host
without those conditions is insufficient. Generic Git/session management can belong to
Hatch later. RAIC bootstrap belongs to an explicit project-owned setup command executed
inside the container; no automatic credential copying belongs in Hatch defaults.
RAIC's documented local cluster mounts only the main checkout and its cluster/image
build tasks reject worktrees, so a worktree is not an isolated running stack. No RAIC
scripts, mounts, worktrees, or cf behavior have been changed by this investigation.

## Supported agent adapters

Codex, Claude, GitHub Copilot CLI, OpenCode, Antigravity CLI, Cursor CLI, and Grok Build
are selectable. Native scripts use official HTTPS endpoints, with separate prerequisite
checks and user-local paths. OpenCode's missing-agent installer selects V2; existing
working V1/V2 installations are never upgraded. Cursor uses cursor-agent instead of the
generic agent alias shared with Grok. Grok's native bin directory is included on PATH
during installation to prevent it replacing Cursor's alias; optional shell-profile setup
is suppressed with SHELL=/bin/sh. Copilot's prefix is explicitly user-local even as root.
Antigravity's output directory is explicit. Vendor installers may configure container
shell state; no host authentication/state is copied. See README for official sources.

Tests cover exact argv and install paths for all seven, official bootstrap routes,
prerequisite failures, partial download rejection/cleanup, one-time navigation hints,
automatic fresh Herdr workspaces, child recursion prevention, --here and pipe handling.
New agent installation routes use disposable script fixtures, not real installations;
provider authentication/model calls for new adapters have not been acceptance-tested.

0.3.0 validation adds config identity, default-Cancel and unattended behavior, startup
result/failure checks, and exact stdin/argv/PID forwarding through startup fixtures.
Live reuse was verified against RAIC with startup disabled by an acceptance guard and
container Python as the probe. Claude was missing from the current container at this
check; the earlier agent acceptance results above describe the previous container state.
No real container was created/rebuilt and no agent was installed to test this feature.

0.4.0 live acceptance: two automatic Herdr launches created distinct owned workspaces,
executed a Python marker inside the existing container, and exited without recursive
handoffs. Only those two workspaces were closed. No new agents were installed to test.

0.5.0 regression coverage includes the default single-container selection, explicit-option
bypass, pasted config/workspace discovery, JSONC and multi-root selection, manual paths
with spaces, cancellation and invalid paths, terminal diagnostic formatting, and agent
installation consent/failure before any Herdr call. Worktree behavior remains deferred.

## Repository development environment (0.5.1)

The local repository has a main branch, README, and root AGENTS.md. Its own devcontainer
uses the official Rust Bookworm image plus explicit Python 3 provisioning and Node.js 22.
The setup hook installs Rust lint/format components, fetches Cargo.lock dependencies,
and installs the pinned resolver under tooling. It mounts only the source workspace,
not Docker's socket or host credentials. Default tests use fixtures; live discovery
runs on the Docker host. Config resolution and shell syntax were validated without
building or starting a container, so an image-build acceptance run remains outstanding.

Inspect remains a single resolved context/object; containers --json is an array of
metadata-discovered running devcontainers, not all Docker services. Bare interactive
launch still prompts for the container even when that inventory has one entry.
Doctor now renders availability and login columns with green checks, red failure
markers, and yellow unknown/unchecked markers. Plain/no-color variants remain usable,
and machine JSON does not change. Authentication uncertainty is not shown as success.
