# Herdr integration

[← Back to README](../README.md)

Hatch launches the agent in the **current terminal** by default, including inside
legacy Herdr and Herdr Core. There is no window/tab destination question. After
session confirmation and any installation/setup, Hatch replaces itself with
`docker exec` and the agent appears directly in your terminal.

Codex may ask you to trust the container workspace or log in. Those are agent
prompts for you to answer; Hatch does not bypass them.

## Explicit handoffs

The advanced CLI options remain for existing workflows:

```sh
hatch --container my-devcontainer --herdr
hatch --container my-devcontainer --herdr-workspace WORKSPACE_ID
```

Inside Core (`HC_TERMINAL_ID`), `--herdr` creates an independent Core session and
immediately attaches to it in the current terminal. Ctrl-\ detaches; the session
keeps running. Hatch execs `hc attach`, leaving no Hatch wrapper. With redirected
input/output or `--no-tty`, the session remains detached and Hatch prints its attach
command. This does not create a desktop window or tab.

Inside legacy Herdr (`HERDR_ENV=1`), `--herdr` creates a workspace and
`--herdr-workspace ID` creates a fresh tab in the named workspace. These explicit
legacy operations preserve focus. Core takes precedence when both markers exist;
Core cannot accept `--herdr-workspace` and never falls back to the legacy server.
`--here` remains supported as an explicit spelling of the default behavior.

Only explicit handoffs probe terminal-manager availability. Capability checks and
workspace validation happen before review; terminal creation waits until confirmation,
agent preparation and setup succeed. Unavailable handoffs resolve locally before review.
A failure after confirmation never silently changes the selected destination or retries
an ambiguous launch. An attach failure includes the address of the retained Core session.

## Child command and diagnostics

Children receive explicit container, workspace, user, agent, configuration and argument
inputs, the prepared executable path when known, `--here`, and `--no-input`. They never
repeat installation, setup or rebuild. Docker connection settings and the launching
shell's host PATH are preserved. Relative PATH entries and resolver file paths are
anchored before changing directories, so a terminal server's stale environment cannot
hide the Dev Container CLI or its Node.js interpreter.

The session review shows the public CLI equivalent; its normal default has no Herdr
summary row. Explicit handoffs include their destination. Arbitrary non-UTF-8 agent
arguments remain supported by direct launch; handoffs require UTF-8.

Observed user-created Core sessions t16–t18 reached Codex inside the container and
waited at its folder-trust screen. They were invisible because the former automatic
handoff created detached Core terminals rather than desktop tabs. Default launches now
stay local, and explicit interactive Core handoffs attach immediately. Existing sessions
are not closed or answered automatically.
