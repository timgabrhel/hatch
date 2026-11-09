# Hatch design and implementation status

Updated 2026-11-06. The package version is defined in Cargo.toml. Generic container-side worktree management is
implemented. RAIC initialization remains project-owned and explicitly selected.

## Documentation map

[README.md](README.md) leads with archive installation and quick-start paths for
interactive and direct CLI launch, followed by a command overview. Source-build instructions live in the development guide.
Detailed user guidance lives in [docs/](docs/usage.md):
[installation](docs/installation.md), [guided launch](docs/usage.md),
[CLI reference](docs/cli.md), [configuration](docs/configuration.md),
[agents](docs/agents.md), [worktrees](docs/worktrees.md), [Herdr](docs/herdr.md),
and [development](docs/development.md).

This file retains architecture, implementation status, and historical validation.
Native archives include the README, these guides, and contributor instructions.

## Goals and boundaries

- Find or start/create devcontainers from config, then launch agents; Codex default, five other agents selectable.
- Main-workspace launch needs no worktree. Resume is explicit agent argument forwarding.
- Dynamic discovery across projects, images, Compose services, mounts, and remote Docker.
- Polished inline terminal interaction, explicit unattended commands, no full-screen UI.
- Small synchronous Rust implementation. Current-terminal launch replaces Hatch with
  Docker using Unix exec. No Hatch daemon, watch loop, or resident launcher.
- Launch stays in the current terminal by default. Explicit Herdr handoffs remain supported.
- Install agents only inside the selected container as the selected development user.
  Check tools and permissions; never assume Bash, package management, or root.
- No automatic host credential/history transfer, devcontainer edits, forced rebuilds,
  implicit worktree mutations, or installed cf replacement. Explicit startup runs project hooks.

## Implemented surface

| Command/option | Behavior |
|---|---|
| `hatch` | Guided container/agent selection in terminals; explicit target required in scripts |
| `update` | Update the running host executable from the latest verified release |
| `up --config PATH` | Reuse a running match, otherwise official CLI start/create, then launch |
| `containers [--json]` | Interactive browser; JSON/nonterminal/--no-input is read-only inventory |
| `inspect [--json]` | Select a container; launch context, agent availability and executable paths |
| `--agent`, `--agent-path` | Agent choice or explicit in-container executable |
| `--container`, `--workspace`, `--user` | Explicit target overrides |
| `--config`, `--host-workspace` | Explicit local source config for resolver/remote Docker |
| `--devcontainer-cli`, `--require-config`, `--metadata-only` | Resolver selection and fallback policy |
| `--install` | Install only if missing; unattended opt-in; no automatic upgrade |
| `--no-input`, `--no-tty`, `--plain` | Prompt, terminal allocation, accessibility controls |
| `--here` | Explicit current-terminal launch (the default) |
| `--herdr`, `--herdr-workspace` | Advanced explicit handoff/target overrides |
| `worktrees [--json]` | Read-only list of Git worktrees inside the selected container |
| `worktree create/remove` | Explicit Git lifecycle in a persistent container mount; no forced removal |
| `--worktree PATH` | Select a registered checkout by absolute container path |
| `--setup-command`, `--init-skill` | Explicit project setup or Codex/Claude initialization prompt |
| `options` | Full settings explained |
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
Only variable names are serialized by inspect. Raw resolver/probe error output
is withheld because it can contain secrets. Config-declared localEnv substitutions
remain supported; there is no additional host credential synchronization.

Launch and inspect honor userEnvProbe, defaulting to loginInteractiveShell for merged
configs. The selected UID's home/shell are detected through passwd; supported probe
shells are sh/bash/dash/ash/zsh/ksh. Shell startup runs as that user, then remoteEnv is
applied. Inspect uses the same configured shell probing for executable checks;
container inventory does not run shell profiles. Containers without
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

`agents.rs` detects six agents for `inspect`, which combines launch context, availability
and executable paths. JSON includes agent, available and executable fields under the
agents array. Authentication and mount-coverage fields/text are removed. Inspect runs
read-only locate/version probes; inventory does not. The redundant doctor, agents and
menu commands have been removed. Bare hatch is the guided launch entry point.
Agent scans use bounded standard-library scoped threads, one per supported agent.
Locate and version remain sequential for each agent; independent agents run concurrently.
The agents table and progress notice appear before environment preparation, with rows
streamed in completion order. Inspect shares the parallel scanner; JSON remains ordered
by Agent::ALL and is emitted only when the scan completes. No async runtime, daemon,
background scan after exit, or production polling loop is introduced. Probe processes
still use synchronous waits within workers; a hung probe can delay scan completion,
but cannot hide the results of other agents. Ctrl-C remains the terminal cancellation.
Authentication probes, the auth column, and the JSON authentication field have been
removed at the user's request. Login and access checks belong to the agent itself.
Antigravity and its agy alias/installer have been removed at the user's request.
Inspect uses launch shell probing but does not install, log in, invoke a model,
or weaken sandboxing.
Persistence is an explicit devcontainer decision.

## Menu UX

Interactive flows start with a once-per-invocation heading and a short description:
Launch agent in container or Inspect container. Plain mode retains these
headings; JSON output omits them. Back navigation does not repeat the flow heading.

Interactive default launch always selects a container, including a singleton. CLI target
options bypass that step; explicit --agent bypasses the agent step.
A custom --agent-path skips agent choice. Unattended default launches require a
container/config target, even with one running container. `inspect` also prompts for a
container even for a singleton, skips selection with explicit target options, and requires
an explicit target unattended. Its table shows name, availability and selection flag.

The container browser's “New from devcontainer.json…” option offers local configs discovered from the current
directory (direct devcontainer.json, .devcontainer.json, .devcontainer/devcontainer.json,
and .devcontainer/PROFILE/devcontainer.json), plus manual path entry. It does not search
parent projects or arbitrary descendants. Multiple matches remain an explicit choice;
no match opens path entry directly.

Config-based launches choose the agent after the workspace and before provisioning,
so build/startup never precedes the agent choice. Explicit --agent/--agent-path still
bypass selection. Cancellation at this stage performs no startup or agent installation.
The compact startup summary shows config, workspace and selected agent under a colored
heading before confirmation. Summary labels are aligned without interior blank lines;
plain/no-color modes remain supported. The redundant no-matching-container sentence
and lifecycle-hook explanation are omitted from the startup prompt. Startup still
executes the project's normal configured lifecycle hooks.

The repository includes an empty hatch.code-workspace placeholder with folders: [].
It intentionally specifies no folders yet. Interactive selection of an empty workspace
uses the config's inferred root without rewriting the file, or asks for a concrete folder
when no root can be inferred. Empty means a blank file, an object without folders, or an
empty folders array; malformed JSON and invalid nonempty folder entries remain errors.
Existing files show only their relative paths; creation choices use a Create prefix.
If no workspace files are found and a project root is inferred, a separate Create option
writes an empty <project>.code-workspace using exclusive creation and returns that root.
Opening/canceling the menu or choosing the project folder creates nothing. Creation
failures surface as errors; existing files are never overwritten.

`workspace_input.rs` searches
current-directory, config-directory and inferred-root .code-workspace files, lists them
as VS Code workspaces with relative paths including the extension and no duplicate canonical
paths. Missing workspace files produce an explicit notice even when an inferred folder
is available. The menu offers a standard inferred folder,
and accepts manual file/folder input. JSONC comments/trailing commas are normalized without
altering quoted strings, then serde_json validates syntax. Relative folder paths resolve
against the workspace file, multiple roots require selection, unsupported URI/variable
paths are explicit errors. No editor tasks/settings are executed. Flags accept a workspace
file too; multi-root unattended usage requires an explicit chosen directory.

Dialoguer/console keep inline arrow-key menus, paging, Back/Esc and terminal restoration.
Navigation help appears once. All selection and review menus omit Back rows; Escape/q returns instead.
Single-line menus, including agent selection, have no
five-item page limit; path-detail menus retain it. A stylized lowercase wordmark becomes HATCH on narrow
terminals, with the Cargo package version below the tagline. Choices separate the
name from an indented, dimmed Path line; plain mode retains the same text hierarchy.
Escape returns through config/workspace/agent/checkout menus and exits
at the first available step. Initialization Back restores the original checkout
context without deleting worktrees. Numbered menus also read Escape immediately.
Path prompts read keys through console so Escape immediately returns to the
previous menu, including in plain mode; partial input is discarded and terminal settings
are restored. Empty Enter or q also goes back. Paths accept quotes and ~/, without shell
execution. The Your session review identifies container/workspace/user/agent, with destination only for an explicit handoff,
pending installation, the initialization mode (including skipped setup), the selected
command or skill path, and agent arguments. Interactive launches
require Install Agent & Launch for a missing agent, or Confirm and launch for an
available agent, before installation, setup, Herdr terminal creation or agent execution.
The summary is part of the arrow menu redraw area, with explicit wrapping so Back
clears all rows. Numbered mode reuses an unchanged summary in its transcript. The review contains only its launch action; Escape returns to the preceding choice.
Accepting the rich review preserves its summary and CLI equivalent in the transcript.
Unattended --no-input launches retain their explicit-command behavior. Notices
and errors wrap into short blocks; inspect renders readable terminal summaries,
with --json or redirected output preserving full JSON. Plain and NO_COLOR modes remain.

## Herdr

Normal launch always stays in the current terminal, including inside legacy Herdr and
Core. The destination menu and default Herdr summary row have been removed at the user's
request. Direct launch still execs Docker and retains exact agent arguments and stdin.
Escape from review returns to checkout, agent or container selection as appropriate.

Advanced --herdr and --herdr-workspace remain explicit opt-ins. Core detection uses
HC_TERMINAL_ID and takes precedence over legacy HERDR_ENV; Core never routes to legacy.
Explicit Core handoffs validate the CLI and current terminal before review, create an
independent session with run --bg --no-parent --no-notify after confirmation/preparation,
and exec hc attach when stdin/stdout are terminals and --no-tty is absent. Redirected
handoffs print the attach command. Attach failures retain and identify the new session.
No Hatch wrapper, duplicate retry or desktop window/tab creation is implied. Legacy
explicit handoffs retain workspace/tab creation and preserve focus. No commands are
sent to existing panes. Normal launch does not probe either terminal manager.

The public CLI equivalent preserves inputs without the internal handoff environment
wrapper. Children receive absolute Hatch/config/workspace paths, the selected agent,
--here and --no-input, and no repeated install/setup. Docker connection settings
and the caller's PATH are pinned; relative PATH entries and resolver paths are anchored
before changing cwd. This fixes the observed t14 failure to find devcontainer in the
Core server's stale environment. A process test executes actual child argv against
Docker/resolver fixtures with a stale server PATH and different cwd for both backends.

Read-only inspection of user-created Core sessions t16–t18 confirmed Codex running in
the selected container and waiting at its folder-trust prompt. The user confirmed local
launch works; the perceived new-window failure was detached Core sessions without desktop
placement. Defaults now avoid that handoff entirely. No trust prompts were answered and
no existing sessions were closed. Tests cover default local launch without Herdr calls,
review gating and Back navigation, explicit Core create-then-attach, child recursion
prevention, and failures. Live commands were inspection only; no test sessions were created.

## Packaging and migration

At the user's request, Hatch's own development config mounts the host's Codex auth.json,
Claude .credentials.json, and GitHub CLI hosts.yml read-only at their normal paths
under /home/vscode. There is no import script or credential copying. Agents cannot
persist token refreshes to these files; host file replacement requires remounting.
An onCreate hook changes ownership of only the three mount parent directories to
vscode, since Docker creates missing mount parents as root; it never recursively
changes ownership or touches the read-only credential files.
The GitHub CLI Feature and gh auth setup-git provide Git HTTPS credentials. This is an
explicit project exception, not automatic credential sharing by the Hatch runtime.
Sources must exist on the local Docker host; keychain-only and SSH auth are not forwarded.
Existing containers must be recreated to gain these mounts; no live rebuild was performed.

The per-user installer atomically installs Hatch alongside cf. Piped installs select
and verify the latest GitHub archive for the host architecture; local installs use the
packaged or built binary and launch unless --no-launch is supplied. PATH is checked
before offering a shell-profile change, which requires explicit consent and is
idempotent. Prompts use /dev/tty for curl | sh; --no-input skips them. Missing resolver
setup is offered when Node.js/npm are present. Existing resolver commands are retained.

`hatch update` runs the embedded installer against the current executable's directory,
without Docker, resolver setup, profile changes, or launch. Equal/newer local versions
are retained. Downloads, checksums, and executable versions are validated before atomic
replacement. It never fetches and executes a mutable installer script from main.

Native Linux archives include binary, docs, installer, resolver lockfile, and dependency
notices, with sidecar checksums and deterministic archive metadata. Builds are dynamically
linked and not guaranteed portable across all Linux distributions.

The Tars release design is adapted to Cargo: manual patch/minor/major or exact-version
selection updates Cargo.toml/Cargo.lock in a release PR; approved checks gate merging
the validated head. Version changes on main create an immutable tag and explicitly
dispatch release.yml. Both native Linux architectures, release process tests, and archive
smoke tests gate draft publication. Published assets cannot be overwritten. Python
standard-library helpers are CI tooling only; the CLI remains synchronous Rust.

cf remains unchanged, including its project-specific worktree behavior. Replacing cf
or adding compatibility shims is outside the generic worktree implementation.

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

## Worktree implementation

The user resumed worktree implementation. `worktrees.rs` runs Git inside the selected
container as its intended user. This supports host-shared repositories and container-only
volumes without guessing that Docker daemon paths are local. Interactive launches offer
current checkout, existing worktrees, creation, removal and Escape cancellation.
Creation explains that the new checkout folder holds the branch files inside the
container, and shows the current checkout and the persistent mount containing it. Explicit
workspace/worktree inputs skip selection. Management commands never provision containers
or launch/install agents. Inspect/inventory do not gain Git probes.

NUL-delimited porcelain parsing preserves spaces/newlines in UTF-8 container paths.
Selection verifies registration, checkout root, common Git metadata, recursive submodule
status and persistent mount coverage. Mount reuse on future rebuilds remains project-owned. Creation canonicalizes existing parent ancestors to reject symlink escapes,
requires persistent checkout/metadata storage and matching relative bind topology,
validates branch/ref inputs, and never overwrites existing paths or fetches. Git 2.48+
uses relative links; old Git may use absolute links only with identical mapped paths or
container-only volumes. Relative links can enable the repository compatibility extension;
accessible local Git is checked, while remote host versions cannot be verified. Standard
Git hooks remain possible. No mount configuration or Git repair is attempted.

Optional setup is a user-selected shell command or initialization skill. Repository
`.agents/skills/*/SKILL.md` files are listed from the main clone first, without interpreting
their contents. Standard Codex/Claude accept one generated initial prompt, only when the
agent tail is empty; other agents keep explicit argv forwarding. The prompt requests init
of the existing checkout, not creation. A skill handoff is not a success assertion. Shell
setup runs once in the parent after agent preparation, with stdin null and progress on
stderr; failure opens no Herdr terminal. The child gets the selected cwd and final prompt,
not setup/create flags. Direct launch still execs Docker.

Removal refuses main/current, locked, missing, dirty, untracked and ignored-file cases,
retains branches, and delegates to Git without --force. Populated submodules can require
manual handling. Confirmation explains that Hatch cannot detect all active sessions;
--yes explicitly acknowledges this. No exit cleanup, branch deletion or automatic prune.
Creation/setup failures retain resources for review. Tests use only disposable repositories,
including local-file submodules, Docker/Herdr doubles, and PTYs. No actual RAIC worktrees,
existing panes, project hooks, agents or mounts are changed for tests.

The copied RAIC `raic-worktree` skill confirms a main-clone script with `init <path>` for
externally created worktrees, submodule and dependency setup, main-clone build limits,
and retained branches. Users can select that skill or explicitly call its init command;
none of its paths or bootstrap behavior is hardcoded into Hatch. The script itself was
not supplied and was not executed. Linux portability work remains on hold. Hatch does not maintain users'
devcontainer.json or infer credential persistence.

## References

- [Dev Container specification](https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md)
- [Official resolver implementation](https://github.com/devcontainers/cli/blob/main/src/spec-node/devContainersSpecCLI.ts)
- [Docker exec](https://docs.docker.com/reference/cli/docker/container/exec/)
- [Rust Unix exec](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.exec)
- Herdr's installed `--help` and `--skill` are the authority for the installed API.

## Earlier RAIC worktree findings (historical)

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
without those conditions is insufficient. These findings informed the generic container-side implementation above. RAIC bootstrap belongs to an explicit project-owned setup command executed
inside the container; no automatic credential copying belongs in Hatch defaults.
RAIC's documented local cluster mounts only the main checkout and its cluster/image
build tasks reject worktrees, so a worktree is not an isolated running stack. No RAIC
scripts, mounts, worktrees, or cf behavior have been changed by this investigation.

## Supported agent adapters

Codex, Claude, GitHub Copilot CLI, OpenCode, Cursor CLI, and Grok Build
are selectable. Native scripts use official HTTPS endpoints, with separate prerequisite
checks and user-local paths. OpenCode's missing-agent installer selects V2; existing
working V1/V2 installations are never upgraded. Cursor uses cursor-agent instead of the
generic agent alias shared with Grok. Grok's native bin directory is included on PATH
during installation to prevent it replacing Cursor's alias; optional shell-profile setup
is suppressed with SHELL=/bin/sh. Copilot's prefix is explicitly user-local even as root.
Vendor installers may configure container
shell state; no host authentication/state is copied. See [agent documentation](docs/agents.md#supported-agents) for official sources.

Tests cover exact argv and install paths for all six, official bootstrap routes,
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
installation consent/failure before any Herdr call. Worktrees were deferred at 0.5.0;
the current implementation is described above.

## Repository development environment (0.5.1)

The local repository has a main branch, README, and root AGENTS.md. Its own devcontainer
uses the official Rust Bookworm image and Node.js 22 directly, without a custom Dockerfile.
The setup hook installs Rust lint/format components, fetches Cargo.lock dependencies,
and installs the pinned resolver under tooling. It mounts only the source workspace,
not Docker's socket; the explicit read-only auth mounts are described above. Default
tests use fixtures; live discovery runs on the Docker host. Development builds, strict
Clippy, formatting, tests, native packaging and release process tests have now passed
inside the running Hatch devcontainer. No container rebuild was performed by the agent.

Inspect remains a single resolved context/object; containers --json is an array of
metadata-discovered running devcontainers, not all Docker services. Bare interactive
launch still prompts for the container even when that inventory has one entry.
Agents renders executable availability with green checks and red failure markers.
Plain/no-color variants remain usable. Authentication diagnostics were subsequently
removed, including the JSON authentication field.

## Rust-only test and packaging tooling

Python tests, executable fixtures, packaging, and live-test scripts have been replaced
with Rust. `cargo test --locked` includes process integration tests and real PTY menu
tests alongside unit tests. The fixture executable records exact OS argument bytes,
stdin, PID, and exit status; independent temporary HOME/PATH environments prevent
accidental use of host state or real services. Installer checks use disposable download
payloads for all six agents, including partial-download cleanup and missing prerequisites.
Related assertions are grouped into table-driven tests; the prior behavioral cases remain.

`cargo run --locked --example package` builds only Hatch for the archive, gathers
dependency license notices, and uses GNU tar/gzip/sha256sum for stable metadata and
checksums. CI runs the Rust suite against debug and release binaries. The devcontainer
no longer explicitly installs Python; its base image may include unrelated tooling.

The two live acceptance checks are ignored Rust tests requiring explicit container/user
environment variables. They use container shell tools instead of a Python probe. These
replacement live checks have not been run against a real container during this migration;
the historical live results above describe earlier implementations.

Migration validation: 68 default tests pass against debug and release Hatch binaries;
the two live tests remain ignored. Formatting and Clippy pass. Native archive checksum,
extraction, executable version, and repeat-build reproducibility were verified. The
official Dev Container CLI resolves the simplified configuration successfully; no
container was built or rebuilt. The installed cf wrapper remains unchanged.

Current-directory discovery and path Escape regressions are covered by PTY fixtures:
all four config layouts, multiple-profile cancellation, manual path overrides, Escape
with partially entered config/workspace paths, plain/rich modes and terminal restoration.

Local paths in container/config/workspace menus and selection summaries are relative
only to the invocation directory (including ../ outside it). JSON, resolver arguments,
and container-internal paths remain absolute. Menus and selection summaries have blank
lines separating their titles, choices and selected paths. Config discovery is bounded
by the documented standard locations, not a recursive directory walk.


## Current worktree validation

The default suite passes 102 tests (16 unit, 71 existing CLI, 2 installer, and 13 new
worktree integration tests); two opt-in live checks remain ignored. Worktree tests cover
creation from explicit refs and existing branches, nested paths, spaces, cancellation,
read-only JSON, launch argv/cwd, main/dirty/ignored/locked removal refusal, populated
submodules, symlink escapes, mismatched mount topology, broken Git links, container
volumes, remote endpoint fixtures, older Git with identical paths, main-clone skill
selection, and parent-only setup before Herdr. No live RAIC acceptance is claimed.
