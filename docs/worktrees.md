# Git worktrees

[← Back to README](../README.md)

For a Git repository, interactive launch adds a **checkout / worktree** choice after
container and agent selection: keep the current checkout, select an existing checkout,
create one, or remove one. This is separate from selecting a folder in a VS Code
`.code-workspace` file.

Explicit `--workspace` or `--worktree` skips the checkout picker;
unattended launches keep their existing working directory unless `--worktree` is supplied.

## Commands

```sh
hatch worktrees --container my-devcontainer --json
hatch --container my-devcontainer --worktree /workspaces/project.worktrees/topic
# Attach an existing local branch:
hatch worktree create /workspaces/project.worktrees/topic --branch topic --container my-devcontainer
# Create a new branch from an explicit existing ref (no fetch):
hatch worktree create /workspaces/project.worktrees/topic --branch topic --from origin/main --container my-devcontainer
hatch worktree remove /workspaces/project.worktrees/topic --container my-devcontainer --yes
```

When creating a worktree, **New checkout folder** is where the branch's files will
live, using the path visible inside the selected container. Hatch shows the current
checkout and its saved project folder to help you choose a location. Enter a new folder under
a persistent mount; Hatch creates it after confirmation. This does not create another
container. Escape revisits the preceding choice; there are no Back menu rows.

## Container-side Git

All paths above are **container paths**. Git runs inside the selected container as its
intended development user; host Git is not used to mutate the repository. Management
requires an existing running container and never starts one, installs agents, fetches,
or creates terminals.

Unattended management requires `--container` or `--config`.
`worktrees --json` returns an array with `path`, `head`, `branch`, `bare`, `locked`, and
`prunable` fields. Paths are absolute; `head`/`branch` can be null.

## Storage and Git compatibility

Creation requires an existing bind mount or named volume covering both the checkout
and its shared Git metadata. Reusing those mounts on future rebuilds remains the
project's responsibility; current coverage alone cannot prove future persistence.

Nested destination directories are supported; an existing
destination is never reused or overwritten. Host-shared mounts must preserve the same
relative layout on the host and in the container.

Relative Git links require Git 2.48+
in both environments; creating them may enable Git's `relativeWorktrees` repository
extension, making the repository incompatible with older Git. Hatch checks local Git
when the mapped source paths are actually accessible; remote host Git compatibility is
the user's responsibility.

With older Git, absolute links are allowed only for identical
host/container paths or container-only volumes. Hatch checks existing checkout links
inside the container before selecting them, but does not repair existing links or edit
mounts. Normal Git hooks may run when creating a checkout.

## Initialization

After selecting or creating a worktree, **Run project setup before launching?**
defaults to **Skip project setup**. The **Your session** review shows that choice,
or the selected command/skill and how it will run, before launch.

Choose an explicit project shell command or ask Codex/Claude to
read an initialization skill. The skill picker lists `.agents/skills/*/SKILL.md` in the
main and selected checkout, preferring the main clone; another absolute container path
can be entered. Skills are offered by path, not automatically executed or interpreted
by Hatch. To request initialization explicitly:

```sh
hatch --container my-devcontainer --worktree /workspaces/project.worktrees/topic \
  --setup-command './scripts/init-worktree.sh'
hatch --container my-devcontainer --worktree /workspaces/project.worktrees/topic \
  --init-skill /workspaces/project/.agents/skills/project-init/SKILL.md
```

`--setup-command` explicitly runs POSIX `sh -c` once inside the selected directory, with
stdin disconnected, after agent preparation and before any Herdr terminal is created.
Failure stops launch and retains the checkout.

`--init-skill` instead supplies one initial
natural-language prompt asking the agent to follow the skill's existing-worktree init
workflow. It supports the standard Codex and Claude executables, conflicts with arguments
after `--` and `--setup-command`, and verifies that the skill file is readable.

Other agents can receive a user-written initialization prompt after `--`. A prompt handoff is
not proof initialization succeeded; the agent reports its result.

No submodule update, dependency installation, credential copying, or editor opening is added automatically.

## Removal and cleanup

Removal requires an explicit confirmation, or `--yes` acknowledging that no session is
using the checkout. It refuses the main/current checkout, locked/unavailable worktrees,
and tracked, untracked, ignored, or submodule changes. Git may also refuse populated
submodules. Hatch never forces removal, deletes branches, prunes, or cleans up after an
agent exits.

Hatch cannot reliably detect every active session. Remove files or resolve
submodule state yourself when needed; failed creation/setup leaves resources intact.

Worktrees share repository history and the selected container; they are not isolated
application stacks. Selection changes the agent's working directory; the selected
container configuration and its environment still apply. `cf` and project-specific
worktree scripts remain unchanged.
