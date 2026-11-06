use crate::{
    cli::{Agent, Cli},
    discovery::{self, Container},
    prompt, provision, workspace_input, Result,
};

pub fn select_from(cli: &mut Cli, mut previous: Option<(Container, Cli)>) -> Result<Container> {
    let original_config = cli.config.clone();
    let original_workspace = cli.host_workspace.clone();
    let can_go_back = cli.container.is_none() && original_config.is_none();
    'containers: loop {
        cli.config = original_config.clone();
        cli.host_workspace = original_workspace.clone();
        let mut agent_selected = false;
        let resuming_config = previous
            .as_ref()
            .is_some_and(|(_, state)| state.config.is_some());
        let container = if let Some((container, state)) = previous.take() {
            *cli = state;
            container
        } else if cli.container.is_none() && cli.config.is_none() {
            let containers = discovery::discover(None)?;
            let mut labels = containers
                .iter()
                .map(Container::display_label)
                .collect::<Vec<_>>();
            if !cli.metadata_only {
                labels.push("New from devcontainer.json…".into());
            }
            let choice = prompt::choose("Select a container", &labels, true)?;
            if let Some(container) = containers.get(choice) {
                container.clone()
            } else {
                loop {
                    let Some(path) = choose_config()? else {
                        continue 'containers;
                    };
                    if !path.is_file()
                        || !path
                            .file_name()
                            .is_some_and(|n| n == "devcontainer.json" || n == ".devcontainer.json")
                    {
                        prompt::warning(
                            "Choose an existing devcontainer.json or .devcontainer.json file.",
                        );
                        continue 'containers;
                    }
                    cli.host_workspace = original_workspace.clone();
                    cli.config = Some(path.canonicalize().map_err(|e| e.to_string())?);
                    agent_selected = true;
                    match configured(cli) {
                        Ok(container) => break container,
                        Err(error) if error.starts_with("cancelled") => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
        } else if cli.config.is_some() && cli.container.is_none() {
            agent_selected = true;
            configured(cli)?
        } else {
            discovery::select(&discovery::discover(cli.container.as_deref())?, true)?.clone()
        };
        if agent_selected || cli.agent_explicit || cli.agent_path.is_some() {
            return Ok(container);
        }
        let labels = Agent::ALL
            .iter()
            .map(|a| a.label().to_owned())
            .collect::<Vec<_>>();
        let default = Agent::ALL.iter().position(|a| *a == cli.agent).unwrap_or(0);
        let choice = match prompt::choose_default("Select an agent", &labels, true, Some(default)) {
            Err(error) if error.starts_with("cancelled") && resuming_config => {
                cli.host_workspace = original_workspace.clone();
                match configured(cli) {
                    Ok(container) => return Ok(container),
                    Err(e) if e.starts_with("cancelled") && can_go_back => continue 'containers,
                    Err(e) => return Err(e),
                }
            }
            Err(error) if error.starts_with("cancelled") && can_go_back => continue 'containers,
            other => other?,
        };
        if let Some(agent) = Agent::ALL.get(choice) {
            cli.agent = *agent;
        } else {
            continue 'containers;
        }
        return Ok(container);
    }
}

// Search only the chosen working directory and standard profile locations, not
// arbitrary descendants or parent projects. Discovery never executes config hooks.
fn local_configs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = vec![
        root.join(".devcontainer/devcontainer.json"),
        root.join(".devcontainer.json"),
        root.join("devcontainer.json"),
    ];
    let mut profiles = std::fs::read_dir(root.join(".devcontainer"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("devcontainer.json"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    profiles.sort();
    paths.extend(profiles);
    paths.retain(|path| path.is_file());
    paths
}

fn choose_config() -> Result<Option<std::path::PathBuf>> {
    let root = std::env::current_dir().map_err(|e| format!("Current directory: {e}"))?;
    let candidates = local_configs(&root);
    loop {
        if !candidates.is_empty() {
            let mut labels = candidates
                .iter()
                .map(|path| prompt::local_path(path))
                .collect::<Vec<_>>();
            labels.push("Paste another config path…".into());
            let choice = match prompt::choose_default(
                "Select a devcontainer config",
                &labels,
                true,
                Some(0),
            ) {
                Ok(choice) => choice,
                Err(error) if error.starts_with("cancelled") => return Ok(None),
                Err(error) => return Err(error),
            };
            if let Some(path) = candidates.get(choice) {
                return Ok(Some(path.clone()));
            }
        }
        let text = prompt::input("devcontainer.json path")?;
        if text.is_empty() || text == "q" {
            if candidates.is_empty() {
                return Ok(None);
            }
            continue;
        }
        return Ok(Some(workspace_input::pasted_path(&text)));
    }
}

fn configured(cli: &mut Cli) -> Result<Container> {
    let config = cli
        .config
        .as_ref()
        .unwrap()
        .canonicalize()
        .map_err(|e| format!("Config: {e}"))?;
    cli.config = Some(config.clone());
    prompt::section("Config selected", &[("File", &prompt::local_path(&config))]);
    let original_workspace = cli.host_workspace.clone();
    'workspace: loop {
        cli.host_workspace = original_workspace.clone();
        if cli.host_workspace.is_none() {
            cli.host_workspace = Some(workspace_input::choose(&config)?);
        }
        if !cli.agent_explicit && cli.agent_path.is_none() {
            let mut labels = Agent::ALL
                .iter()
                .map(|a| a.label().to_owned())
                .collect::<Vec<_>>();
            labels.push("Back".into());
            let default = Agent::ALL.iter().position(|a| *a == cli.agent).unwrap_or(0);
            let choice =
                match prompt::choose_default("Select an agent", &labels, true, Some(default)) {
                    Ok(choice) if choice < Agent::ALL.len() => choice,
                    Ok(_) => {
                        if original_workspace.is_none() {
                            continue 'workspace;
                        }
                        return Err("cancelled; nothing launched".into());
                    }
                    Err(e) if e.starts_with("cancelled") && original_workspace.is_none() => {
                        continue 'workspace
                    }
                    Err(e) => return Err(e),
                };
            cli.agent = Agent::ALL[choice];
        }
        break 'workspace;
    }
    discovery::select(&provision::discover(cli, true)?, true).cloned()
}

#[cfg(test)]
mod tests {
    use super::local_configs;
    #[test]
    fn discovery_is_bounded_to_current_directory_and_standard_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        for relative in [
            "project/devcontainer.json",
            "project/.devcontainer.json",
            "project/.devcontainer/devcontainer.json",
            "project/.devcontainer/rust/devcontainer.json",
            "project/.devcontainer/rust/deeper/devcontainer.json",
            "project/child/devcontainer.json",
            "devcontainer.json",
        ] {
            let path = temp.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "{}").unwrap();
        }
        assert_eq!(
            local_configs(&root),
            [
                root.join(".devcontainer/devcontainer.json"),
                root.join(".devcontainer.json"),
                root.join("devcontainer.json"),
                root.join(".devcontainer/rust/devcontainer.json"),
            ]
        );
    }
}
