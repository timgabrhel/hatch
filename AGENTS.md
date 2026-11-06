# Working on Hatch

Read `DESIGN.md` for requirements, implementation status, and deferred decisions.
Use `README.md` for the public command overview and `docs/cli.md` for the full
command contract. Keep the relevant guides in `docs/` and `DESIGN.md` aligned with changes.

## Development

Hatch is a small synchronous Rust CLI for launching coding agents in devcontainers.
Agent inventory uses bounded scoped threads for concurrent read-only checks, with
streamed terminal results and deterministic JSON ordering. Keep direct launch as exec.
The development environment is in `.devcontainer/`; current stable Rust and standard
Linux shell tools are sufficient for default tests. Node.js/npm are needed only for
the optional pinned Dev Container CLI in `tooling/`.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

For release-binary process tests, set `HATCH_TEST_BINARY=target/release/hatch` after
`cargo build --release --locked`, then run `cargo test --locked --tests`.
`cargo run --locked --example package` builds a native archive in `dist/`, using GNU
tar, gzip, and sha256sum. Build products and local dependency installations are ignored.

## Commits

- Use Conventional Commits: `type(scope): short imperative summary`.
- Choose a clear type such as `feat`, `fix`, `docs`, `refactor`, `test`, or `ci`;
  include a scope when it helps identify the affected area.
- Keep the subject concise, preferably within 72 characters, without a trailing period.
- Separate independently reviewable changes into focused commits. Keep implementation,
  tests, and directly related documentation together when practical.
- After a blank line, use short `-` bullets for the concrete changes and relevant
  validation. Do not write paragraph-style commit bodies.
- Mark breaking changes with `!` and a `BREAKING CHANGE:` footer when applicable.

## Code map

- `cli.rs`, `main.rs`: arguments and execution flow.
- `menu.rs`, `prompt.rs`, `workspace_input.rs`: inline menus and local workspace input.
- `discovery.rs`, `configuration.rs`, `provision.rs`: Docker metadata, resolved config,
  and explicitly requested startup through the official Dev Container CLI.
- `install.rs`, `src/scripts/`: container-local agent discovery and installation.
- `launch.rs`, `herdr.rs`: terminal execution and optional terminal-manager handoff.
- `agents.rs`: concurrent container-specific executable checks used by inspect.
- `worktrees.rs`: container-side Git lifecycle, checkout selection and explicit initialization.
- `tests/`: disposable Docker, resolver, installer, and Herdr fixtures; opt-in live checks.
  Cargo builds the Rust `hatch-test-fixture` helper for integration tests; it is never
  included in release archives or installed by `scripts/install.sh`.

## Behavioral constraints

- Default interactive launch asks for the container, even when only one is running.
  Explicit options bypass the corresponding selection; unattended launch needs a target.
- Use metadata and `devcontainer.json`; do not add RAIC-specific paths or configuration.
- Preserve agent arguments as OS strings. Direct launch replaces Hatch with Docker;
  no persistent Hatch process, daemon, or polling loop.
- Prepare the agent before creating a Herdr terminal. Child handoffs carry explicit
  target/agent inputs, `--here`, and `--no-input` to prevent recursion and repeat prompts.
- In Herdr, prompt for new workspace or this workspace (new tab) as the last choice before review,
  after container startup and checkout selection. Show installation required in the Your session
  summary and use Install Agent & Launch as its final action for missing agents.
  Available agents use Confirm and launch; do not show a separate install prompt. Setup and handoff also
  wait for confirmation. Back/Escape revisits choices; --no-input skips review.
  Read the current ID from HERDR_WORKSPACE_ID; explicit destination flags bypass.
- Detect prerequisites and install only in the selected container as its intended user.
  Do not copy host credentials or agent state, or silently edit devcontainer configuration.
  Explicit exception: this repository's devcontainer exposes the three user-requested
  host auth files directly via read-only mounts at the container user's auth paths. Keep this
  project setup separate from Hatch's runtime defaults; test only with dummy credentials.
- Keep inspect/inventory read-only. Container startup requires explicit `up` or consent
  in the guided flow. Never force a rebuild or remove a container as a fallback.
- Inspect checks executable availability and paths only; do not add authentication or mount-coverage
  probes or infer login status from installation. Login belongs to the agent itself.
- `inspect` prompts for a container even when there is only one; unattended use requires
  --container or --config. Inspect includes read-only executable probes; inventory does not.
- Display local paths relative to the current directory. Keep JSON and actual process
  arguments absolute, and do not reinterpret container-internal paths as host paths.
- Keep menus inline and accessible: plain mode, no-color support, Back/cancel, restored
  terminal settings, concise notices, and JSON output for scripts.

## Testing and scope

Use fixtures and disposable directories for automated tests. Do not create/remove real
worktrees, rebuild live containers, or install agents merely to test a change. Live tests
must be explicit, scoped, and clean up only resources they created. Never send commands
to an existing agent pane for a test. Preserve the installed `cf` script.

Worktree management is explicitly user-requested. Keep Git operations container-side as
the intended user, require persistent mounts and valid Git links, retain branches, and
never force removal or automatically clean up after sessions. Do not generalize RAIC
bootstrap or credential-copying behavior into Hatch defaults. Use disposable Git
repositories for tests; Git on PATH is required alongside the standard shell tools.

Do not publish, push, or add a remote unless requested. Local repository changes and
ordinary validation do not require an extra confirmation step.
