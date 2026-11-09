# Guided launch

[← Back to README](../README.md)

```sh
hatch
```

Hatch guides you through **container → agent → checkout → review**,
skipping steps that do not apply. The container picker appears even with one running
container. Choose **New from devcontainer.json…** to start from a project config.

For direct launch, specify your choices and skip Hatch's prompts:

```sh
hatch --container my-devcontainer --agent claude --no-input --here
```

Add `--install` to allow installation of a missing agent. The agent can still ask
its own questions. See the [CLI reference](cli.md) for all options.

## Project and workspace selection

Hatch discovers standard devcontainer configs in the current directory and offers
workspace files and the inferred project folder. You can also enter a path; quotes
and `~/` are supported.

A matching running container is reused. Startup requires confirmation and runs the
project's normal lifecycle hooks. Agent selection happens before startup. The action
says **Start container** for an existing match or **Create container** for a new one;
you review the session afterward before launching the agent. Startup logs appear in
a compact pane (Up/Down to scroll, f to follow). Failures include a full log file path.
Plain mode streams output normally.

Workspace files support JSON comments, trailing commas, and relative folders.
Multi-folder files prompt for a folder. Empty workspace files use the inferred root;
Hatch only creates a workspace file when you select the explicit create action.
See [configuration](configuration.md) for discovery and remote Docker details.

For replacement, choose **Rebuild a devcontainer… → target container → agent →
Rebuild container**. Rebuild logs appear next, followed by checkout selection and
session review. The target list includes stopped devcontainers. Cancelling before
rebuild confirmation performs no replacement; backing out afterward cannot undo it.

## Agent and checkout

The agent picker shows availability. Missing agents can be installed as part of launch.
For Git projects, choose the current checkout, an existing worktree, or an explicit
create/remove action. Initialization is optional. See [agents](agents.md) and
[worktrees](worktrees.md).

The agent opens in your current terminal, including inside Herdr/Core. There is no
window/tab destination choice. Explicit Herdr CLI options remain available for advanced
workflows; see [Herdr](herdr.md).

## Review and launch

**Your session** summarizes the target, agent, arguments, and initialization:
skipped setup, a command to run, or the agent and skill path selected.
Choose **Confirm and launch**, or **Install Agent & Launch** when the agent is missing.
Escape returns to the preceding choice; menus have no Back rows. Installation, setup, and terminal handoff wait for this
confirmation; container startup and worktree changes have their own earlier confirmations.

## Navigation and accessibility

Use arrow keys and Enter to select; Escape goes back and exits at the first menu.
In path prompts, empty Enter or `q` also goes back. Terminal settings are restored
when a menu closes.

| Setting | Effect |
|---|---|
| `--plain`, `HATCH_PLAIN=1`, or `TERM=dumb` | Numbered prompts. |
| `NO_COLOR` | No color. |
| `--no-input` | No Hatch prompts or review; required choices must be explicit. |
| `--no-tty` | No Docker terminal allocation; Hatch prompts are controlled separately. |

The accepted session summary and CLI equivalent remain in the terminal transcript.
Agent installation uses the same scrolling log pane as container startup (Up/Down
or k/j to scroll, f to follow). After installation, Hatch displays the summary again,
including on failure; successful installation changes the agent status to Installed.
Full installer stdout and stderr are saved to a private `hatch-install-*.log` in the
system temporary directory. Installation and verification failures include its path.
Plain mode streams installer output and then repeats the summary.
