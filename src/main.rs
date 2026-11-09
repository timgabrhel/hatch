mod agents;
mod cli;
mod configuration;
mod discovery;
mod herdr;
mod install;
mod launch;
mod menu;
mod process;
mod prompt;
mod provision;
mod update;
mod workspace_input;
mod worktrees;

use clap::{CommandFactory, FromArgMatches};
use cli::{Action, Cli};
use std::ffi::OsString;
use std::io::IsTerminal;

type Result<T> = std::result::Result<T, String>;

fn run(mut cli: Cli, mut tail: Vec<OsString>) -> Result<()> {
    if matches!(cli.command, Some(Action::Update)) {
        return update::run(&cli, &tail);
    }
    if matches!(cli.command, Some(Action::Options)) {
        Cli::command()
            .mut_args(|a| a.hide(false))
            .print_long_help()
            .map_err(|e| e.to_string())?;
        println!("\n\nTerminal features (TTY) are automatic: arrow keys, colors, resizing and Ctrl-C.\n--no-tty disables them for scripts. --no-input disables Hatch questions only.\n--plain uses numbered questions; --here stays in the current terminal.\nThese settings do not change the agent's own permissions or prompts.");
        return Ok(());
    }
    let interactive = prompt::available(cli.no_input);
    worktrees::check_options(&cli, &tail)?;
    prompt::set_plain(cli.plain);
    if interactive && matches!(cli.command, Some(Action::Containers { json: false })) {
        cli.command = None;
    }
    if interactive {
        match cli.command {
            None | Some(Action::Up) => prompt::workflow(
                "Launch agent in container",
                "Choose where to work and which coding agent to run.",
            ),
            Some(Action::Inspect { json: false }) => prompt::workflow(
                "Inspect container",
                "Choose a container to review its workspace and available agents.",
            ),
            Some(Action::Worktrees { json: false } | Action::Worktree { .. }) => prompt::workflow(
                "Manage worktrees",
                "Manage Git checkouts inside the selected container.",
            ),
            _ => {}
        }
    }
    if let Some(path) = &cli.host_workspace {
        cli.host_workspace = Some(workspace_input::folder(path, interactive)?);
    }
    if !tail.is_empty()
        && matches!(
            cli.command,
            Some(Action::Containers { .. } | Action::Inspect { .. })
        )
    {
        return Err("agent arguments after -- are only accepted for launch".into());
    }
    if (cli.install || cli.herdr || cli.herdr_workspace.is_some())
        && matches!(
            cli.command,
            Some(Action::Containers { .. } | Action::Inspect { .. })
        )
    {
        return Err("--install and --herdr are only valid for launch".into());
    }
    // Validate local arguments before contacting Docker.
    if let Some(path) = cli.workspace.as_deref() {
        discovery::absolute_path(path)?;
    }
    launch::executable(&cli)?;
    provision::validate(&cli)?;
    let guided = interactive && matches!(cli.command, None | Some(Action::Up));
    let initial_cli = cli.clone();
    let mut previous = None;
    let mut checkout_back = None;
    'selection: loop {
        let selected = if guided {
            if let Some(container) = checkout_back.take() {
                container
            } else {
                menu::select_from(&mut cli, previous.take())?
            }
        } else {
            let containers = provision::discover(&mut cli, interactive)?;
            if let Some(Action::Containers { json }) = cli.command {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &containers
                                .iter()
                                .map(discovery::Container::summary)
                                .collect::<Vec<_>>()
                        )
                        .map_err(|e| e.to_string())?
                    );
                } else {
                    println!("Running devcontainers\n");
                    for container in &containers {
                        println!(
                            "  {}\n    Project: {}\n",
                            prompt::clean(container.name.trim_start_matches('/')),
                            prompt::clean(
                                &container
                                    .local_folder()
                                    .map(|p| prompt::local_path(std::path::Path::new(p)))
                                    .unwrap_or_else(|| "unknown".into())
                            )
                        );
                    }
                    println!("Run hatch to choose a container or paste a devcontainer.json path.");
                }
                return Ok(());
            }
            if !interactive
                && (matches!(cli.command, Some(Action::Inspect { .. }))
                    || worktrees::is_action(&cli))
                && cli.container.is_none()
                && cli.config.is_none()
            {
                return Err(
                    "Choose the target explicitly: pass --container NAME or --config PATH.".into(),
                );
            }
            let selected = if interactive && cli.container.is_none() && cli.config.is_none() {
                let labels = containers
                    .iter()
                    .map(discovery::Container::display_label)
                    .collect::<Vec<_>>();
                containers[prompt::choose("Select a container", &labels, true)?].clone()
            } else {
                discovery::select(&containers, interactive)?.clone()
            };
            if !interactive
                && cli.command.is_none()
                && cli.container.is_none()
                && cli.config.is_none()
            {
                return Err("Choose the target explicitly.\nRun hatch in a terminal, or pass --container NAME / --config PATH for scripts.".into());
            }
            selected
        };
        let mut context = configuration::resolve(&cli, &selected, interactive)?;
        if worktrees::is_action(&cli) {
            return worktrees::manage(&cli, &context, &selected, interactive);
        }
        if let Some(Action::Inspect { json }) = cli.command {
            let json_output = json || !std::io::stdout().is_terminal();
            if !json_output {
                println!("Launch context (one container)\n");
                for (label, value) in [
                    ("Container", context.container_name.as_str()),
                    ("Workspace", context.workspace.as_str()),
                    ("User", context.user.as_str()),
                    ("Source", context.configuration_source.as_str()),
                ] {
                    println!("  {label:<12} {}", prompt::clean(value));
                }
                if let Some(path) = &context.config_file {
                    println!(
                        "  Config       {}",
                        prompt::clean(&prompt::local_path(path))
                    );
                }
                if !context.remote_env_names.is_empty() {
                    println!("\nEnvironment variable names (values hidden):");
                    for name in &context.remote_env_names {
                        println!("  {}", prompt::clean(name));
                    }
                }
            }
            let mut probe_context = context.clone();
            let agents = agents::scan(&cli, &mut probe_context, json_output)?;
            if json_output {
                let mut report = serde_json::to_value(&context).map_err(|e| e.to_string())?;
                report["agents"] = serde_json::json!(agents);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
                );
            } else {
                println!("\nFull metadata: hatch inspect --json");
                for warning in &context.warnings {
                    prompt::warning(warning);
                }
            }
            return Ok(());
        }
        for warning in &context.warnings {
            prompt::warning(warning);
        }
        let before_checkout = cli.clone();
        let checkout_shown =
            match worktrees::choose(&mut cli, &mut context, &selected, &tail, interactive) {
                Ok(shown) => shown,
                Err(error) => {
                    if guided && error.starts_with("cancelled") {
                        if !cli.agent_explicit && cli.agent_path.is_none() {
                            previous = Some((selected, cli.clone()));
                            cli = initial_cli.clone();
                            continue;
                        }
                        if initial_cli.container.is_none() && initial_cli.config.is_none() {
                            cli = initial_cli.clone();
                            continue;
                        }
                    }
                    return Err(error);
                }
            };
        // Menu selections may have introduced an initialization option.
        worktrees::check_options(&cli, &tail)?;
        configuration::prepare_env(&mut context)?;
        let agent_plan = install::prepare(&cli, &context)?;
        let destination = herdr::prepare_destination(&mut cli, &context)?;
        if interactive {
            let agent_action = match &agent_plan {
                install::PreparedAgent::Available(_) => "Installed",
                install::PreparedAgent::Install(_) => "! Install",
            };
            let initialization = if cli.setup_command.is_some() {
                "Run setup command before launch".to_owned()
            } else if cli.init_skill.is_some() {
                format!("Ask {} to follow the selected skill", cli.agent.label())
            } else {
                "Skip project setup".to_owned()
            };
            let mut fields: Vec<(&str, &str)> = vec![
                ("Container", &context.container_name),
                ("Workspace", &context.workspace),
                ("User", &context.user),
                ("Agent", cli.agent.label()),
                ("Agent status", agent_action),
                ("Initialization", &initialization),
            ];
            if destination.is_some() {
                fields.push(("Herdr", herdr::destination(&cli)));
            }
            if let Some(command) = &cli.setup_command {
                fields.push(("Setup command", command));
            }
            if let Some(skill) = &cli.init_skill {
                fields.push(("Initialization skill", skill));
            }
            let arguments = tail
                .iter()
                .map(|arg| format!("{arg:?}"))
                .collect::<Vec<_>>()
                .join(" ");
            if !arguments.is_empty() {
                fields.push(("Agent arguments", &arguments));
            }
            let mut repeat = cli.clone();
            repeat.install = matches!(agent_plan, install::PreparedAgent::Install(_));
            let command = herdr::preview(&repeat, &context, &tail)?;
            fields.push(("CLI equivalent", &command));
            let confirmed = match prompt::review(
                &fields,
                matches!(agent_plan, install::PreparedAgent::Install(_)),
            ) {
                Ok(confirmed) => confirmed,
                Err(e) if e.starts_with("cancelled") => false,
                Err(e) => return Err(e),
            };
            if !confirmed {
                if checkout_shown {
                    cli = before_checkout;
                    checkout_back = Some(selected);
                    continue 'selection;
                }
                if !cli.agent_explicit && cli.agent_path.is_none() {
                    previous = Some((selected, cli.clone()));
                    cli = initial_cli.clone();
                    continue 'selection;
                }
                if initial_cli.container.is_none() && initial_cli.config.is_none() {
                    cli = initial_cli.clone();
                    continue 'selection;
                }
                return Err("cancelled; nothing launched".into());
            }
        }
        let executable = install::execute(agent_plan, &cli, &context)?;
        worktrees::prepare(&cli, &context, &mut tail)?;
        if let Some(destination) = &destination {
            let mut child = cli.clone();
            child.install = false;
            child.no_input = true;
            if std::path::Path::new(&executable).is_absolute() {
                child.agent_path = Some(executable.clone());
            }
            if herdr::launch(destination, &child, &context, &tail)? {
                return Ok(());
            }
        }
        return launch::run(&cli, &context, &tail, &executable);
    }
}

fn main() {
    let (own, tail) = cli::split_args(std::env::args_os());
    let matches = Cli::command().get_matches_from(own);
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    cli.agent_explicit =
        matches.value_source("agent") == Some(clap::parser::ValueSource::CommandLine);
    if let Err(error) = run(cli, tail) {
        // Keep readable line breaks while escaping terminal control sequences.
        let message: String = error
            .chars()
            .flat_map(|c| {
                if c.is_control() && c != '\n' && c != '\t' {
                    c.escape_default().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect();
        eprintln!("\n  Hatch could not continue\n");
        prompt::note(&message);
        std::process::exit(1);
    }
}
