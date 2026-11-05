# Working on Hatch

Read `DESIGN.md` for requirements, implementation status, and deferred decisions.
Use `README.md` for the public command contract. Keep both aligned with changes.

## Development

Hatch is a small synchronous Rust CLI for launching coding agents in devcontainers.
The development environment is in `.devcontainer/`; current stable Rust and Python 3
are sufficient for default tests. Node.js/npm are needed only for the optional pinned
Dev Container CLI in `tooling/`.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
python3 -m unittest discover -s tests -v
```

For release-binary process tests, set `HATCH_TEST_BINARY=target/release/hatch` after
`cargo build --release --locked`. `scripts/package.py` builds a native archive in
`dist/`. Build products, caches, and local dependency installations are ignored.

## Code map

- `cli.rs`, `main.rs`: arguments and execution flow.
- `menu.rs`, `prompt.rs`, `workspace_input.rs`: inline menus and local workspace input.
- `discovery.rs`, `configuration.rs`, `provision.rs`: Docker metadata, resolved config,
  and explicitly requested startup through the official Dev Container CLI.
- `install.rs`, `src/scripts/`: container-local agent discovery and installation.
- `launch.rs`, `herdr.rs`: terminal execution and optional terminal-manager handoff.
- `doctor.rs`: availability/authentication diagnostics without credential output.
- `tests/`: disposable Docker, resolver, installer, and Herdr fixtures; opt-in live checks.

## Behavioral constraints

- Default interactive launch asks for the container, even when only one is running.
  Explicit options bypass the corresponding selection; unattended launch needs a target.
- Use metadata and `devcontainer.json`; do not add RAIC-specific paths or configuration.
- Preserve agent arguments as OS strings. Direct launch replaces Hatch with Docker;
  no persistent Hatch process, daemon, or polling loop.
- Prepare the agent before creating a Herdr terminal. Child handoffs carry explicit
  target/agent inputs, `--here`, and `--no-input` to prevent recursion and repeat prompts.
- Detect prerequisites and install only in the selected container as its intended user.
  Do not copy host credentials or agent state, or silently edit devcontainer configuration.
- Keep inspect/inventory read-only. Container startup requires explicit `up` or consent
  in the guided flow. Never force a rebuild or remove a container as a fallback.
- Distinguish missing/broken agents, authenticated, unavailable auth status, and unchecked
  auth. Do not infer successful authentication from installation or an unknown result.
- Keep menus inline and accessible: plain mode, no-color support, Back/cancel, restored
  terminal settings, concise notices, and JSON output for scripts.

## Testing and scope

Use fixtures and disposable directories for automated tests. Do not create/remove real
worktrees, rebuild live containers, or install agents merely to test a change. Live tests
must be explicit, scoped, and clean up only resources they created. Never send commands
to an existing agent pane for a test. Preserve the installed `cf` script.

Worktree implementation is deferred until the user resumes that discussion. Do not
generalize RAIC bootstrap or credential-copying behavior into Hatch defaults.

Do not publish, push, or add a remote unless requested. Local repository changes and
ordinary validation do not require an extra confirmation step.
