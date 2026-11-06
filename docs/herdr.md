# Herdr integration

[← Back to README](../README.md)

Inside Herdr (`HERDR_ENV=1`), interactive launches offer two destinations:

- **New workspace**, named after the selected container.
- **This workspace (new tab)**, using `HERDR_WORKSPACE_ID`.

Destination selection is the last choice before **Your session**. The review then
asks for **Confirm and launch** or **Install Agent & Launch**, depending on agent availability.
Backing out does not install an agent or create a Herdr terminal.

## Choose a destination explicitly

```sh
hatch --here
hatch --herdr
hatch --herdr-workspace WORKSPACE_ID
```

`--here` keeps the current terminal. `--herdr` requests a new workspace, while
`--herdr-workspace ID` opens a fresh tab in a specific workspace. These options
skip the destination question.

Hatch reads the current workspace ID from `HERDR_WORKSPACE_ID`, not another client's
focused workspace. It validates that ID before creating a tab.

## Handoff behavior

Hatch prepares the agent before creating a fresh terminal. It submits an explicit
Hatch command and exits, leaving existing panes untouched and preserving focus.
An acknowledgement means the command was queued, not that the agent finished starting.

The child command includes:

- Explicit container, workspace, user, agent, arguments, and config inputs.
- The prepared absolute agent path when available.
- `--here` to prevent recursion and `--no-input` to skip another review.
- The selected Docker connection settings.

The child rechecks the executable but never installs it again. Changes to the
container between preflight and child startup can still cause a failure.

Arguments are shell-quoted once. Non-UTF-8 arguments fail before workspace creation;
use `--here` for arbitrary Unix argument bytes.

After an ambiguous pane-run failure, Hatch identifies the created terminal. It does
not retry the handoff or launch a duplicate locally. No workspace reuse registry is
used; old Hatch registry files are left untouched.

## Local fallback

Outside Herdr, or when Herdr is unavailable, Hatch uses the current terminal.
Handoff also needs a verified local project directory and a local Unix Docker endpoint.
Without those, Hatch falls back locally.

Piped input and redirected output normally stay local so scripts retain their data.
An explicit `--herdr` request can override automatic destination detection.

Local launch uses Unix `exec`: Hatch replaces itself with Docker. Input, signals,
resizing, and the exit status go directly through Docker, while the calling shell
waits normally. Hatch leaves no resident parent, daemon, or polling loop.
