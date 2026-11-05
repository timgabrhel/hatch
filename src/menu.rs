use crate::{
    cli::{Action, Agent, Cli},
    discovery::{self, Container},
    prompt, provision, workspace_input, Result,
};

pub fn select(cli: &mut Cli) -> Result<Container> {
    prompt::heading();
    let force_agent = matches!(cli.command, Some(Action::Menu));
    let original_config = cli.config.clone();
    let original_workspace = cli.host_workspace.clone();
    let can_go_back = cli.container.is_none() && original_config.is_none();
    'containers: loop {
        cli.config = original_config.clone();
        cli.host_workspace = original_workspace.clone();
        let container = if cli.container.is_none() && cli.config.is_none() {
            let containers = discovery::discover(None)?;
            let mut labels = containers
                .iter()
                .map(Container::display_label)
                .collect::<Vec<_>>();
            if !cli.metadata_only {
                labels.push("Use devcontainer.json…".into());
            }
            let choice = prompt::choose("Select a container", &labels, true)?;
            if let Some(container) = containers.get(choice) {
                container.clone()
            } else {
                let text = prompt::input("devcontainer.json path")?;
                if text.is_empty() || text == "q" {
                    continue;
                }
                let path = workspace_input::pasted_path(&text);
                if !path.is_file()
                    || !path
                        .file_name()
                        .is_some_and(|n| n == "devcontainer.json" || n == ".devcontainer.json")
                {
                    prompt::warning(
                        "Choose an existing devcontainer.json or .devcontainer.json file.",
                    );
                    continue;
                }
                cli.config = Some(path.canonicalize().map_err(|e| e.to_string())?);
                match configured(cli) {
                    Ok(container) => container,
                    Err(error) if error.starts_with("cancelled") => continue,
                    Err(error) => return Err(error),
                }
            }
        } else if cli.config.is_some() && cli.container.is_none() {
            configured(cli)?
        } else {
            discovery::select(&discovery::discover(cli.container.as_deref())?, true)?.clone()
        };
        prompt::section(
            "Container selected",
            &[("Container", container.name.trim_start_matches('/'))],
        );
        if (!force_agent && cli.agent_explicit) || cli.agent_path.is_some() {
            return Ok(container);
        }
        let mut labels = Agent::ALL
            .iter()
            .map(|a| a.label().to_owned())
            .collect::<Vec<_>>();
        if can_go_back {
            labels.push("Back to containers".into());
        }
        let default = Agent::ALL.iter().position(|a| *a == cli.agent).unwrap_or(0);
        let choice = match prompt::choose_default("Select an agent", &labels, true, Some(default)) {
            Err(error) if error.starts_with("cancelled") && !prompt::plain() && can_go_back => {
                continue 'containers
            }
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

fn configured(cli: &mut Cli) -> Result<Container> {
    let config = cli
        .config
        .as_ref()
        .unwrap()
        .canonicalize()
        .map_err(|e| format!("Config: {e}"))?;
    cli.config = Some(config.clone());
    if cli.host_workspace.is_none() {
        cli.host_workspace = Some(workspace_input::choose(&config)?);
    }
    discovery::select(&provision::discover(cli, true)?, true).cloned()
}
