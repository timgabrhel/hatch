//! Configuration identity, startup and explicitly requested rebuilds.
use crate::{
    cli::{Action, Cli},
    configuration,
    discovery::{self, Container},
    prompt, Result,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn validate(cli: &Cli) -> Result<()> {
    if matches!(cli.command, Some(Action::Up | Action::Rebuild { .. })) {
        let action = if matches!(cli.command, Some(Action::Rebuild { .. })) {
            "rebuild"
        } else {
            "up"
        };
        if cli.config.is_none() {
            return Err(format!("{action} requires --config PATH (and --host-workspace DIR for nonstandard config layouts)"));
        }
        if cli.container.is_some() {
            return Err(format!("{action} selects by configuration; omit --container, or omit {action} to attach to that explicit container"));
        }
    }
    Ok(())
}

pub(crate) fn workspace(config: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    let folder = if let Some(folder) = explicit {
        folder.to_owned()
    } else if config
        .file_name()
        .is_some_and(|n| n == ".devcontainer.json")
    {
        config.parent().unwrap().to_owned()
    } else if let Some(dir) = config
        .ancestors()
        .skip(1)
        .find(|dir| dir.file_name().is_some_and(|n| n == ".devcontainer"))
    {
        dir.parent()
            .ok_or("cannot infer workspace; supply --host-workspace")?
            .to_owned()
    } else {
        return Err("Nonstandard config location; supply --host-workspace DIR rather than guessing the project root".into());
    };
    if !folder.is_dir() {
        return Err("--host-workspace must be an existing directory".into());
    }
    folder
        .canonicalize()
        .map_err(|e| format!("host workspace: {e}"))
}

fn same_path(label: Option<&str>, path: &Path, local: bool) -> bool {
    label.is_some_and(|label| {
        Path::new(label) == path
            || (local && Path::new(label).canonicalize().is_ok_and(|p| p == path))
    })
}

pub fn discover(cli: &mut Cli, interactive: bool) -> Result<Vec<Container>> {
    // An explicit container keeps the existing resolver override semantics.
    if cli.config.is_none() || cli.container.is_some() {
        return discovery::discover(cli.container.as_deref());
    }
    let config = cli.config.as_ref().unwrap();
    if !config.is_file() {
        return Err(format!("Config is not a file: {:?}", config));
    }
    let config = config.canonicalize().map_err(|e| format!("config: {e}"))?;
    let local = configuration::local_docker();
    if !local && cli.host_workspace.is_none() {
        return Err("Remote Docker requires --host-workspace with --config".into());
    }
    let folder = workspace(&config, cli.host_workspace.as_deref())?;
    cli.config = Some(config.clone());
    cli.host_workspace = Some(folder.clone());
    let rebuilding = matches!(cli.command, Some(Action::Rebuild { .. }));
    let mut containers = if rebuilding {
        discovery::discover_all()?
    } else {
        discovery::discover(None)?
    };
    containers.retain(|c| {
        same_path(c.label("devcontainer.config_file"), &config, local)
            && same_path(c.local_folder(), &folder, local)
    });
    if (!rebuilding && !containers.is_empty())
        || matches!(cli.command, Some(Action::Containers { .. }))
    {
        return Ok(containers);
    }
    if matches!(cli.command, Some(Action::Inspect { .. })) || crate::worktrees::is_action(cli) {
        return Err("This config and workspace need a container. Use hatch up --config PATH to start/create it".into());
    }
    let log = crate::startup::prepare_log(false)?;
    let log_path = log.path().display().to_string();
    if rebuilding {
        if !interactive && !cli.no_input {
            return Err(
                "Rebuild needs confirmation; pass --no-input to authorize unattended replacement"
                    .into(),
            );
        }
        if containers.len() != 1 {
            return Err("Rebuild requires exactly one existing container matching the config and workspace; nothing rebuilt".into());
        }
        if interactive {
            prompt::section(
                "Ready to rebuild",
                &[
                    ("Container", &containers[0].name),
                    ("Config", &prompt::local_path(&config)),
                    ("Workspace", &prompt::local_path(&folder)),
                    ("Agent", cli.agent.label()),
                    ("Dev container log", &log_path),
                ],
            );
            prompt::caution("Rebuild stops sessions and deletes files outside persistent mounts.");
            if prompt::choose_default(
                "Rebuild this devcontainer?",
                &["Cancel".into(), "Rebuild container".into()],
                true,
                Some(0),
            )? == 0
            {
                return Err("cancelled; nothing rebuilt".into());
            }
        }
    } else if !matches!(cli.command, Some(Action::Up)) {
        if !interactive {
            return Err("This config and workspace need a container. Use hatch up --config PATH to start/create it (runs project lifecycle hooks)".into());
        }
        let existing = crate::process::capture(
            Command::new("docker")
                .args(["ps", "--all", "--quiet", "--no-trunc"])
                .arg("--filter")
                .arg(format!(
                    "label=devcontainer.config_file={}",
                    config.display()
                ))
                .arg("--filter")
                .arg(format!(
                    "label=devcontainer.local_folder={}",
                    folder.display()
                )),
            "check for an existing devcontainer",
        )?;
        let exists = !existing.iter().all(u8::is_ascii_whitespace);
        prompt::section(
            if exists {
                "Ready to start"
            } else {
                "Ready to create"
            },
            &[
                ("Config", &prompt::local_path(&config)),
                ("Workspace", &prompt::local_path(&folder)),
                ("Agent", cli.agent.label()),
                ("Dev container log", &log_path),
            ],
        );
        prompt::note("After startup, review your session before launching the agent.");
        let choice = prompt::choose_default(
            if exists {
                "Start this devcontainer?"
            } else {
                "Create this devcontainer?"
            },
            &[
                "Cancel".into(),
                if exists {
                    "Start container".into()
                } else {
                    "Create container".into()
                },
            ],
            true,
            Some(0),
        )?;
        if choice == 0 {
            return Err("cancelled; nothing launched".into());
        }
    }
    if !interactive || matches!(cli.command, Some(Action::Up)) {
        prompt::section(
            if rebuilding {
                "Ready to rebuild"
            } else {
                "Ready to build"
            },
            &[
                ("Config", &prompt::local_path(&config)),
                ("Workspace", &prompt::local_path(&folder)),
                ("Agent", cli.agent.label()),
                ("Dev container log", &log_path),
            ],
        );
        if rebuilding {
            prompt::caution("Rebuild stops sessions and deletes files outside persistent mounts.");
        }
    }
    let mut command = Command::new(&cli.devcontainer_cli);
    command
        .args(["up", "--config"])
        .arg(&config)
        .arg("--workspace-folder")
        .arg(&folder)
        .arg("--id-label")
        .arg(format!(
            "devcontainer.local_folder={}",
            folder
                .to_str()
                .ok_or("workspace path must be UTF-8 for Dev Container labels")?
        ))
        .arg("--id-label")
        .arg(format!(
            "devcontainer.config_file={}",
            config
                .to_str()
                .ok_or("config path must be UTF-8 for Dev Container labels")?
        ))
        .arg("--no-lockfile");
    if rebuilding {
        command.args(["--remove-existing-container", "--expect-existing-container"]);
        if matches!(cli.command, Some(Action::Rebuild { no_cache: true })) {
            command.arg("--build-no-cache");
        }
    }
    let (output, log) = crate::startup::run(&mut command, interactive, log)?;
    let result = (|| {
        if !output.status.success() {
            return Err(format!("Dev Container startup failed ({}); nothing launched. Containers created before the failure are left intact", output.status));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Started {
            outcome: String,
            container_id: String,
        }
        let started: Started = serde_json::from_slice(&output.stdout).map_err(|_| {
            "Invalid Dev Container startup result; nothing launched (JSON output withheld)"
        })?;
        if started.outcome != "success"
            || started.container_id.is_empty()
            || !started.container_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(
                "Dev Container startup did not return a successful container ID; nothing launched"
                    .into(),
            );
        }
        let containers = discovery::discover(Some(&started.container_id))?;
        if !containers.iter().all(|c| {
            same_path(c.label("devcontainer.config_file"), &config, local)
                && same_path(c.local_folder(), &folder, local)
        }) {
            return Err("Started container does not match the requested config/workspace labels; nothing launched".into());
        }
        Ok(containers)
    })();
    if result.is_ok() && rebuilding {
        cli.command = None;
    }
    result.map_err(|e: String| format!("{e}. Dev container log: {}", log.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_workspace_layouts_and_no_guesses() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for relative in [
            ".devcontainer.json",
            ".devcontainer/devcontainer.json",
            ".devcontainer/linux/devcontainer.json",
        ] {
            assert_eq!(workspace(&root.join(relative), None).unwrap(), root);
        }
        assert!(workspace(&root.join("custom/config.json"), None).is_err());
        assert_eq!(
            workspace(&root.join("custom/config.json"), Some(&root)).unwrap(),
            root
        );
    }
}
