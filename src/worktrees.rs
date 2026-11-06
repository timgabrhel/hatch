//! Git operations run as the intended user in the selected container. No mount,
//! devcontainer, credential, or project bootstrap configuration is written.
use crate::{
    cli::{Action, Agent, Cli, WorktreeAction},
    discovery::{self, Container, Context},
    launch,
    process::capture,
    prompt, Result,
};
use serde::Serialize;
use std::{
    ffi::OsString,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Serialize)]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

fn git_command(context: &Context, args: &[&str]) -> Command {
    let mut command = launch::base(context);
    launch::target(&mut command, context, "git").args(args);
    command
}
fn git(context: &Context, args: &[&str]) -> Result<Vec<u8>> {
    capture(&mut git_command(context, args), "container Git")
}
fn text(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| "Git paths must be UTF-8 inside the container".into())
}
fn line(bytes: Vec<u8>) -> Result<String> {
    let mut value = text(bytes)?;
    if value.ends_with('\n') {
        value.pop();
    }
    Ok(value)
}
fn at(context: &Context, path: &str) -> Context {
    let mut result = context.clone();
    result.workspace = path.to_owned();
    result
}
fn canonical(context: &Context, path: &str) -> Result<String> {
    discovery::absolute_path(path)?;
    line(capture(
        &mut launch::script(
            context,
            "cd -P \"$1\" && pwd -P",
            "hatch-worktree-path",
            &[path],
        ),
        "resolve container directory",
    )?)
}

fn parse(bytes: Vec<u8>) -> Result<Vec<Worktree>> {
    let text = text(bytes)?;
    let mut result = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in text.split('\0') {
        if field.is_empty() {
            if let Some(row) = current.take() {
                result.push(row);
            }
        } else if let Some(path) = field.strip_prefix("worktree ") {
            discovery::absolute_path(path)?;
            if current.is_some() {
                return Err("Malformed Git worktree record".into());
            }
            current = Some(Worktree {
                path: path.into(),
                head: None,
                branch: None,
                bare: false,
                locked: false,
                prunable: false,
            });
        } else if let Some(row) = current.as_mut() {
            if let Some(head) = field.strip_prefix("HEAD ") {
                row.head = Some(head.into());
            }
            if let Some(branch) = field.strip_prefix("branch refs/heads/") {
                row.branch = Some(branch.into());
            }
            row.bare |= field == "bare";
            row.locked |= field == "locked" || field.starts_with("locked ");
            row.prunable |= field == "prunable" || field.starts_with("prunable ");
        } else {
            return Err("Malformed Git worktree response".into());
        }
    }
    if let Some(row) = current {
        result.push(row);
    }
    if result.is_empty() {
        return Err("Git returned no worktrees".into());
    }
    Ok(result)
}
fn list(context: &Context) -> Result<Vec<Worktree>> {
    parse(git(context, &["worktree", "list", "--porcelain", "-z"])?)
}
fn root(context: &Context) -> Result<String> {
    let root = line(git(context, &["rev-parse", "--show-toplevel"])?)?;
    discovery::absolute_path(&root)?;
    Ok(root)
}
fn common(context: &Context) -> Result<String> {
    let path = line(git(
        context,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?)?;
    canonical(context, &path)
}
fn persistent<'a>(container: &'a Container, path: &str) -> Result<&'a discovery::Mount> {
    let mount = container.mounts.iter()
        .filter(|m| Path::new(path).starts_with(&m.destination))
        .max_by_key(|m| Path::new(&m.destination).components().count())
        .filter(|m| ["bind", "volume"].contains(&m.kind.as_str()))
        .ok_or_else(|| format!("{path:?} is not covered by a persistent bind mount or named volume. Configure the project's mounts yourself; Hatch will not edit devcontainer.json."))?;
    Ok(mount)
}

/// Read from the selected worktree as well as the base repository: broken links
/// must fail before environment setup, installation, or a Herdr mutation.
fn validate(context: &Context, container: &Container, path: &str) -> Result<String> {
    let path = canonical(context, path)?;
    let row = list(context)?
        .into_iter()
        .find(|w| w.path == path)
        .ok_or("The selected path is not a registered worktree of this repository")?;
    if row.bare || row.prunable {
        return Err("The selected worktree is bare or unavailable".into());
    }
    let selected = at(context, &path);
    if root(&selected)? != path || common(&selected)? != common(context)? {
        return Err("The worktree's Git links do not resolve to the selected repository inside the container. Check the project's mounts and Git path layout.".into());
    }
    git(&selected, &["submodule", "status", "--recursive"]).map_err(|e| {
        format!("{e}\nSubmodule Git links are unavailable inside the selected container; initialize or repair them with the project's own tooling.")
    })?;
    persistent(container, &path)?;
    persistent(container, &common(&selected)?)?;
    Ok(path)
}
fn select(context: &mut Context, container: &Container, path: &str) -> Result<()> {
    let path = validate(context, container, path)?;
    context.daemon_host_path = discovery::host_path(&container.mounts, &path);
    context.workspace = path;
    context.workspace_source = "Git worktree selection".into();
    Ok(())
}
fn recent_git(version: &[u8]) -> bool {
    let value = String::from_utf8_lossy(version);
    let mut parts = value
        .trim()
        .strip_prefix("git version ")
        .unwrap_or("")
        .split('.');
    matches!((parts.next().and_then(|v| v.parse::<u32>().ok()), parts.next().and_then(|v| v.parse::<u32>().ok())), (Some(major), Some(minor)) if major > 2 || major == 2 && minor >= 48)
}

fn relative_path(from: &Path, to: &Path) -> std::path::PathBuf {
    let from = from.components().collect::<Vec<_>>();
    let to = to.components().collect::<Vec<_>>();
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut result = std::path::PathBuf::new();
    for _ in &from[shared..] {
        result.push("..");
    }
    for part in &to[shared..] {
        result.push(part);
    }
    result
}

fn link_mode(
    context: &Context,
    container: &Container,
    destination: &str,
    common: &str,
) -> Result<bool> {
    let host_destination = discovery::host_path(&container.mounts, destination);
    let host_common = discovery::host_path(&container.mounts, common);
    if let (Some(destination_host), Some(common_host)) = (&host_destination, &host_common) {
        if relative_path(Path::new(common), Path::new(destination))
            != relative_path(common_host, destination_host)
        {
            return Err("The worktree and Git metadata mounts have different relative layouts on the Docker host and in the container. Configure matching mount topology before creating worktrees.".into());
        }
    } else if host_destination.is_some() || host_common.is_some() {
        return Err("A host-shared worktree and its Git metadata must both have bind mappings; mixed bind/volume layouts cannot preserve shared Git links".into());
    }
    let mut relative = recent_git(&git(context, &["--version"])?);
    let different_paths = host_destination
        .as_ref()
        .is_some_and(|p| p != Path::new(destination))
        || host_common.as_ref().is_some_and(|p| p != Path::new(common));
    // Only claim to check local Git when both source directories are accessible.
    // A local Docker socket alone does not make daemon paths local to Hatch.
    if relative
        && crate::configuration::local_docker()
        && host_common.as_ref().is_some_and(|p| p.is_dir())
        && host_destination
            .as_ref()
            .and_then(|p| p.parent())
            .is_some_and(|p| p.is_dir())
    {
        relative = recent_git(&capture(
            Command::new("git").arg("--version"),
            "check local Git compatibility",
        )?);
    }
    if !relative && different_paths {
        return Err("Different host/container paths require relative worktree links and Git 2.48+ in both environments".into());
    }
    Ok(relative)
}

fn create(
    context: &Context,
    container: &Container,
    path: &str,
    branch: &str,
    start: Option<&str>,
) -> Result<String> {
    discovery::absolute_path(path)?;
    git(context, &["check-ref-format", "--branch", branch])?;
    // Canonicalize the existing parent to catch symlinks into ephemeral storage.
    let normalized = Path::new(path).components().collect::<std::path::PathBuf>();
    let requested = normalized.as_path();
    let parent = requested
        .parent()
        .ok_or("A worktree destination needs a parent")?;
    let parent = line(capture(&mut launch::script(context,
        "parent=$1; suffix=; while ! test -d \"$parent\"; do test ! -e \"$parent\" && test ! -L \"$parent\" || exit 1; name=${parent##*/}; suffix=/$name$suffix; parent=${parent%/*}; test -n \"$parent\" || parent=/; done; cd -P \"$parent\" && printf '%s%s\\n' \"$PWD\" \"$suffix\"",
        "hatch-worktree-parent", &[parent.to_str().ok_or("Invalid destination")?]), "resolve worktree parent")?)?;
    let name = requested
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid destination")?;
    let destination = Path::new(&parent).join(name).to_str().unwrap().to_owned();
    persistent(container, &destination)?;
    let common = common(context)?;
    persistent(container, &common)?;
    let relative = link_mode(context, container, &destination, &common)?;
    capture(
        &mut launch::script(
            context,
            "test ! -e \"$1\" && test ! -L \"$1\"",
            "hatch-worktree-new",
            &[&destination],
        ),
        "worktree destination must not exist",
    )?;
    let source = if let Some(start) = start {
        if branch.starts_with('-') {
            return Err("Invalid branch name".into());
        }
        git(
            context,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{start}^{{commit}}"),
            ],
        )?;
        start.to_owned()
    } else {
        let source = format!("refs/heads/{branch}");
        git(context, &["show-ref", "--verify", &source])?;
        branch.to_owned()
    };
    // A scoped config override avoids adding persistent worktree.useRelativePaths.
    // Git records extensions.relativeWorktrees as needed for the relative links.
    let mut args = vec![
        "-c",
        if relative {
            "worktree.useRelativePaths=true"
        } else {
            "worktree.useRelativePaths=false"
        },
        "worktree",
        "add",
    ];
    if start.is_some() {
        args.extend(["-b", branch]);
    }
    args.extend(["--", &destination, &source]);
    git(context, &args).map_err(|e| format!("{e}\nGit may have left a branch or checkout at {destination}; inspect it before retrying. Hatch did not force cleanup."))?;
    prompt::note(&format!(
        "Created worktree {destination}. Its branch is retained if setup or launch fails."
    ));
    validate(context, container, &destination).map_err(|e| {
        format!("{e}\nThe created worktree remains at {destination}; no cleanup was attempted.")
    })
}

fn remove(
    context: &Context,
    container: &Container,
    path: &str,
    yes: bool,
    interactive: bool,
) -> Result<()> {
    let path = validate(context, container, path)?;
    let worktrees = list(context)?;
    let selected = worktrees.iter().find(|w| w.path == path).unwrap();
    if worktrees.first().is_some_and(|w| w.path == path) {
        return Err("The main checkout cannot be removed".into());
    }
    if root(context)? == path {
        return Err("Cannot remove the current checkout; select the main repository first".into());
    }
    if selected.locked {
        return Err("The worktree is locked; Hatch will not unlock it".into());
    }
    let selected_context = at(context, &path);
    // Include untracked and ignored files; deleting ignored dependencies or secrets
    // silently would be surprising. Git performs its own final cleanliness check.
    let status = git(
        &selected_context,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored",
            "--ignore-submodules=none",
        ],
    )?;
    prompt::section(
        "Remove worktree",
        &[
            ("Path", &path),
            (
                "Branch",
                selected.branch.as_deref().unwrap_or("detached HEAD"),
            ),
            (
                "Status",
                if status.is_empty() {
                    "clean"
                } else {
                    "contains changed, untracked, ignored, or submodule files"
                },
            ),
        ],
    );
    if !status.is_empty() {
        return Err("Refusing to remove a worktree containing files Git would discard. Clean it yourself; Hatch never forces removal.".into());
    }
    prompt::note("Hatch cannot detect every active session. Close agents and terminals using this checkout before removing it. The branch will be kept.");
    if !yes {
        if !interactive {
            return Err(
                "Removal requires --yes to confirm no session is using the checkout".into(),
            );
        }
        if prompt::choose_default(
            "Remove this clean worktree?",
            &["Cancel".into(), "Remove worktree (keep branch)".into()],
            true,
            Some(0),
        )? != 1
        {
            return Err("cancelled; worktree retained".into());
        }
    }
    // Re-read lock/state immediately before mutation. Never --force or prune.
    let selected = list(context)?
        .into_iter()
        .find(|w| w.path == path)
        .ok_or("Worktree disappeared")?;
    if selected.locked || selected.prunable {
        return Err("Worktree state changed; nothing removed".into());
    }
    git(context, &["worktree", "remove", "--", &path])?;
    prompt::note("Worktree removed; its branch was kept.");
    Ok(())
}

pub fn is_action(cli: &Cli) -> bool {
    matches!(
        cli.command,
        Some(Action::Worktrees { .. } | Action::Worktree { .. })
    )
}
pub fn check_options(cli: &Cli, tail: &[OsString]) -> Result<()> {
    if let Some(path) = &cli.worktree {
        discovery::absolute_path(path)?;
    }
    if let Some(path) = &cli.init_skill {
        discovery::absolute_path(path)?;
    }
    if cli.init_skill.is_some() {
        if !matches!(cli.agent, Agent::Codex | Agent::Claude) || cli.agent_path.is_some() {
            return Err("--init-skill currently supports the standard Codex and Claude executables; pass your own prompt after -- for other agents".into());
        }
        if !tail.is_empty() {
            return Err("--init-skill cannot be combined with agent arguments after --; include the initialization request in your own prompt instead".into());
        }
    }
    if is_action(cli)
        && (!tail.is_empty()
            || cli.worktree.is_some()
            || cli.init_skill.is_some()
            || cli.setup_command.is_some()
            || cli.install
            || cli.herdr
            || cli.herdr_workspace.is_some())
    {
        return Err(
            "Worktree management cannot be combined with launch, setup, or agent arguments".into(),
        );
    }
    if matches!(
        cli.command,
        Some(Action::Inspect { .. } | Action::Containers { .. })
    ) && (cli.worktree.is_some() || cli.init_skill.is_some() || cli.setup_command.is_some())
    {
        return Err("--worktree, --init-skill, and --setup-command are launch options".into());
    }
    if let Some(Action::Worktree { ref command }) = cli.command {
        let path = match command {
            WorktreeAction::Create { path, .. } | WorktreeAction::Remove { path, .. } => path,
        };
        discovery::absolute_path(path)?;
    }
    Ok(())
}

pub fn manage(
    cli: &Cli,
    context: &Context,
    container: &Container,
    interactive: bool,
) -> Result<()> {
    match &cli.command {
        Some(Action::Worktrees { json }) => {
            let rows = list(context)?;
            if *json || !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
                );
            } else {
                for row in rows {
                    println!(
                        "{}  {}{}{}",
                        prompt::clean(row.branch.as_deref().unwrap_or("detached HEAD")),
                        prompt::clean(&row.path),
                        if row.locked { " (locked)" } else { "" },
                        if row.prunable { " (unavailable)" } else { "" }
                    );
                }
            }
            Ok(())
        }
        Some(Action::Worktree {
            command:
                WorktreeAction::Create {
                    path,
                    branch,
                    start,
                },
        }) => create(context, container, path, branch, start.as_deref()).map(|_| ()),
        Some(Action::Worktree {
            command: WorktreeAction::Remove { path, yes },
        }) => remove(context, container, path, *yes, interactive),
        _ => Err("Expected a worktree command".into()),
    }
}

fn input(title: &str) -> Result<Option<String>> {
    let value = prompt::input(title)?;
    Ok((!value.is_empty() && value != "q").then_some(value))
}
fn cancelled(error: &str) -> bool {
    error.starts_with("cancelled")
}

fn create_menu(context: &Context, container: &Container) -> Result<Option<String>> {
    let branches = text(git(
        context,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )?)?;
    let mut branches = branches.lines().map(String::from).collect::<Vec<_>>();
    let existing = branches.len();
    branches.extend(["Create a new branch…".into(), "Back".into()]);
    'branch: loop {
        let choice = match prompt::choose_default("Choose a branch", &branches, true, Some(0)) {
            Ok(choice) => choice,
            Err(e) if cancelled(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        if choice == existing + 1 {
            return Ok(None);
        }
        'name: loop {
            let (branch, start) = if choice == existing {
                let Some(branch) = input("New branch name")? else {
                    continue 'branch;
                };
                let Some(start) =
                    input("Starting ref or commit (for example HEAD or origin/main; no fetch)")?
                else {
                    continue 'name;
                };
                (branch, Some(start))
            } else {
                (branches[choice].clone(), None)
            };
            loop {
                let Some(path) = input(
                    "Worktree destination inside container (absolute path on a persistent mount)",
                )?
                else {
                    continue 'branch;
                };
                prompt::section(
                    "Create worktree",
                    &[
                        ("Branch", &branch),
                        ("Path", &path),
                        ("From", start.as_deref().unwrap_or("existing local branch")),
                    ],
                );
                prompt::note("Uses relative Git links with Git 2.48+ (required when host/container paths differ). Git may enable the relativeWorktrees extension. No fetch or extra project bootstrap runs; normal Git hooks may execute.");
                match prompt::choose_default(
                    "Create this worktree?",
                    &["Back".into(), "Create worktree".into()],
                    true,
                    Some(0),
                ) {
                    Ok(1) => {
                        return create(context, container, &path, &branch, start.as_deref())
                            .map(Some)
                    }
                    Ok(_) => return Ok(None),
                    Err(e) if cancelled(&e) => continue,
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

fn initialization_skill(context: &Context) -> Result<Option<String>> {
    let rows = list(context)?;
    let main = &rows.first().ok_or("Missing main checkout")?.path;
    let current = root(context)?;
    // Prefer main-clone skills: a newly created checkout may contain an older copy.
    let bytes = capture(&mut launch::script(context,
        "for root in \"$1\" \"$2\"; do for skill in \"$root\"/.agents/skills/*/SKILL.md; do if test -f \"$skill\"; then printf '%s\\000' \"$skill\"; fi; done; done",
        "hatch-worktree-skills", &[main, &current]), "discover repository initialization skills")?;
    let mut paths = Vec::new();
    for path in text(bytes)?.split('\0').filter(|s| !s.is_empty()) {
        if !paths.iter().any(|p| p == path) {
            paths.push(path.to_owned());
        }
    }
    if !paths.is_empty() {
        let mut labels = paths.clone();
        labels.extend(["Paste another skill path…".into(), "Back".into()]);
        let choice = match prompt::choose_default(
            "Choose an initialization skill",
            &labels,
            true,
            Some(0),
        ) {
            Ok(choice) => choice,
            Err(e) if cancelled(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        if let Some(path) = paths.get(choice) {
            return Ok(Some(path.clone()));
        }
        if choice == paths.len() + 1 {
            return Ok(None);
        }
    }
    let path = input("Initialization SKILL.md path inside container (absolute)")?;
    if let Some(path) = &path {
        discovery::absolute_path(path)?;
    }
    Ok(path)
}

fn setup_menu(cli: &mut Cli, context: &Context, tail: &[OsString]) -> Result<()> {
    if cli.init_skill.is_some() || cli.setup_command.is_some() {
        return Ok(());
    }
    loop {
        let supported = matches!(cli.agent, Agent::Codex | Agent::Claude)
            && cli.agent_path.is_none()
            && tail.is_empty();
        let mut choices = vec![
            "Launch without initialization".into(),
            "Run a project setup command…".into(),
        ];
        if supported {
            choices.push("Ask the agent to read and run an initialization skill…".into());
        }
        choices.push("Back to checkouts".into());
        match prompt::choose_default("Initialize this checkout?", &choices, true, Some(0))? {
            0 => return Ok(()),
            1 => {
                if let Some(command) = input("Project setup command (runs inside this checkout)")? {
                    cli.setup_command = Some(command);
                    return Ok(());
                }
            }
            2 if supported => {
                if let Some(path) = initialization_skill(context)? {
                    cli.init_skill = Some(path);
                    return Ok(());
                }
            }
            _ => return Err("cancelled; worktree retained, nothing launched".into()),
        }
    }
}

pub fn choose(
    cli: &mut Cli,
    context: &mut Context,
    container: &Container,
    tail: &[OsString],
    interactive: bool,
) -> Result<bool> {
    if let Some(path) = cli.worktree.clone() {
        select(context, container, &path)?;
        return Ok(false);
    }
    // Explicit workspace bypasses selection; unattended launch keeps its current cwd.
    if !interactive || cli.workspace.is_some() {
        return Ok(false);
    }
    let original = context.clone();
    let root = match root(context) {
        Ok(root) => root,
        Err(_) => return Ok(false),
    };
    loop {
        *context = original.clone();
        let rows = list(context)?;
        let mut choices = vec![prompt::path_item("Current checkout", &context.workspace)];
        let linked = rows
            .iter()
            .filter(|row| row.path != root && !row.bare)
            .collect::<Vec<_>>();
        for row in &linked {
            choices.push(prompt::path_item(
                &format!(
                    "{}{}",
                    row.branch.as_deref().unwrap_or("detached HEAD"),
                    if row.prunable { " (unavailable)" } else { "" }
                ),
                &row.path,
            ));
        }
        choices.extend([
            "Create worktree…".into(),
            "Remove worktree…".into(),
            "Back".into(),
        ]);
        let choice =
            prompt::choose_default("Choose a checkout / worktree", &choices, true, Some(0))?;
        if choice == 0 {
            return Ok(true);
        }
        if choice <= linked.len() {
            match select(context, container, &linked[choice - 1].path) {
                Ok(()) => {
                    match setup_menu(cli, context, tail) {
                        Ok(()) => {}
                        Err(e) if cancelled(&e) => continue,
                        Err(e) => return Err(e),
                    }
                    return Ok(true);
                }
                Err(e) => {
                    prompt::warning(&e);
                    continue;
                }
            }
        }
        if choice == linked.len() + 1 {
            match create_menu(context, container) {
                Ok(Some(path)) => {
                    select(context, container, &path)?;
                    match setup_menu(cli, context, tail) {
                        Ok(()) => {}
                        Err(e) if cancelled(&e) => continue,
                        Err(e) => return Err(e),
                    }
                    return Ok(true);
                }
                Ok(None) => {}
                Err(e) if cancelled(&e) => {}
                Err(e) => prompt::warning(&e),
            }
        } else if choice == linked.len() + 2 {
            let mut labels = linked
                .iter()
                .map(|w| prompt::path_item(w.branch.as_deref().unwrap_or("detached HEAD"), &w.path))
                .collect::<Vec<_>>();
            labels.push("Back".into());
            let choice = match prompt::choose("Choose a worktree to remove", &labels, true) {
                Ok(i) => i,
                Err(e) if cancelled(&e) => continue,
                Err(e) => return Err(e),
            };
            if let Some(row) = linked.get(choice) {
                if let Err(e) = remove(context, container, &row.path, false, true) {
                    prompt::warning(&e);
                }
            }
        } else {
            return Err("cancelled; nothing launched".into());
        }
    }
}

/// Setup is parent-only. The Herdr child receives the final agent prompt and cwd,
/// never the shell command, so setup cannot repeat after handoff.
pub fn prepare(cli: &Cli, context: &Context, tail: &mut Vec<OsString>) -> Result<()> {
    if let Some(path) = &cli.init_skill {
        capture(
            &mut launch::script(
                context,
                "test -f \"$1\" && test -r \"$1\"",
                "hatch-worktree-skill",
                &[path],
            ),
            "initialization skill must be readable inside container",
        )?;
        tail.push(format!("Read the initialization skill at {path:?} and follow its existing-worktree initialization instructions for the current checkout at {:?}. This checkout already exists: use its init/setup workflow, do not create or switch checkouts. Respect the project's instructions and ask for any approvals the skill requires. Report what you initialized and any failures; do not assume this is an isolated running application stack.", context.workspace).into());
    }
    if let Some(command) = &cli.setup_command {
        prompt::section(
            "Project setup",
            &[("Workspace", &context.workspace), ("Command", command)],
        );
        let status = launch::script(context, command, "hatch-project-setup", &[])
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::io::stderr()))
            .status()
            .map_err(|e| format!("Project setup could not start: {e}; checkout retained"))?;
        if !status.success() {
            return Err(format!(
                "Project setup failed ({status}); checkout retained, no agent terminal created"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn porcelain_handles_unusual_paths_and_flags() {
        let rows = parse(b"worktree /a path\nwith newline\0HEAD abc\0branch refs/heads/main\0\0worktree /other\0HEAD def\0detached\0locked reason\0prunable gone\0\0".to_vec()).unwrap();
        assert_eq!(rows[0].path, "/a path\nwith newline");
        assert_eq!(rows[0].branch.as_deref(), Some("main"));
        assert!(rows[1].locked && rows[1].prunable);
        assert!(parse(b"unexpected\0".to_vec()).is_err());
        assert!(parse(b"worktree relative\0\0".to_vec()).is_err());
        assert!(recent_git(b"git version 2.48.0\n"));
        assert!(!recent_git(b"git version 2.47.9\n"));
    }
}
