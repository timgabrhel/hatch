mod cli;
mod configuration;
mod discovery;
mod doctor;
mod herdr;
mod install;
mod launch;
mod menu;
mod process;
mod prompt;
mod provision;
mod workspace_input;

use clap::{CommandFactory, FromArgMatches};
use cli::{Action, Cli};
use std::ffi::OsString;
use std::io::IsTerminal;

type Result<T> = std::result::Result<T, String>;

fn run(mut cli: Cli, tail: Vec<OsString>) -> Result<()> {
    if matches!(cli.command, Some(Action::Options)) {
        Cli::command()
            .mut_args(|a| a.hide(false))
            .print_long_help()
            .map_err(|e| e.to_string())?;
        println!("\n\nTerminal features (TTY) are automatic: arrow keys, colors, resizing and Ctrl-C.\n--no-tty disables them for scripts. --no-input disables Hatch questions only.\n--plain uses numbered questions; --here stays in the current terminal.\nThese settings do not change the agent's own permissions or prompts.");
        return Ok(());
    }
    if matches!(cli.command, Some(Action::Agents)) {
        println!("Supported agents\n");
        println!("  {:<20} CLI SHORTCUT", "AGENT");
        for agent in cli::Agent::ALL {
            let label = if agent == cli::Agent::Codex {
                "Codex (default)"
            } else {
                agent.label()
            };
            println!("  {:<20} hatch --agent {}", label, agent.key());
        }
        println!("\nRun hatch to choose interactively.\nMissing agents: Hatch asks before installing inside the selected container.");
        return Ok(());
    }
    let interactive = prompt::available(cli.no_input);
    prompt::set_plain(cli.plain);
    if interactive && matches!(cli.command, Some(Action::Containers { json: false })) {
        cli.command = Some(Action::Menu);
    }
    if let Some(path) = &cli.host_workspace {
        cli.host_workspace = Some(workspace_input::folder(path, interactive)?);
    }
    if !tail.is_empty()
        && matches!(
            cli.command,
            Some(Action::Containers { .. } | Action::Inspect { .. } | Action::Doctor { .. })
        )
    {
        return Err("agent arguments after -- are only accepted for launch or menu".into());
    }
    if (cli.install || cli.herdr || cli.herdr_workspace.is_some())
        && matches!(
            cli.command,
            Some(Action::Containers { .. } | Action::Inspect { .. } | Action::Doctor { .. })
        )
    {
        return Err("--install and --herdr are only valid for launch or menu".into());
    }
    // Validate local arguments before contacting Docker.
    if let Some(path) = cli.workspace.as_deref() {
        discovery::absolute_path(path)?;
    }
    launch::executable(&cli)?;
    provision::validate(&cli)?;
    if matches!(cli.command, Some(Action::Menu)) && !interactive {
        return Err("menu requires an interactive terminal; omit menu and provide explicit --container/--workspace/--user options".into());
    }
    let guided = interactive && matches!(cli.command, None | Some(Action::Menu | Action::Up));
    let selected = if guided {
        menu::select(&mut cli)?
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
                        prompt::clean(container.local_folder().unwrap_or("unknown"))
                    );
                }
                println!("Run hatch to choose a container or paste a devcontainer.json path.");
            }
            return Ok(());
        }
        let selected = discovery::select(&containers, interactive)?.clone();
        if !interactive && cli.command.is_none() && cli.container.is_none() && cli.config.is_none()
        {
            return Err("Choose the target explicitly.\nRun hatch in a terminal, or pass --container NAME / --config PATH for scripts.".into());
        }
        selected
    };
    let mut context = configuration::resolve(&cli, &selected, interactive)?;
    if let Some(Action::Inspect { json }) = cli.command {
        if json || !std::io::stdout().is_terminal() {
            println!(
                "{}",
                serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?
            );
        } else {
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
                    prompt::clean(&path.display().to_string())
                );
            }
            println!("\nEnvironment variable names (values hidden):");
            for name in &context.remote_env_names {
                println!("  {}", prompt::clean(name));
            }
            println!("\nFull metadata: hatch inspect --json");
            println!("All discovered devcontainers: hatch containers --json");
            for warning in &context.warnings {
                prompt::warning(warning);
            }
        }
        return Ok(());
    }
    if let Some(Action::Doctor { json }) = cli.command {
        configuration::prepare_env(&mut context)?;
        return doctor::run(
            &cli,
            &context,
            &selected,
            json || !std::io::stdout().is_terminal(),
        );
    }
    for warning in &context.warnings {
        prompt::warning(warning);
    }
    let automatic_herdr = !cli.here
        && std::env::var("HERDR_ENV").as_deref() == Ok("1")
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    let handoff = cli.herdr || cli.herdr_workspace.is_some() || automatic_herdr;
    if interactive {
        prompt::section(
            "Your session",
            &[
                ("Container", &context.container_name),
                ("Workspace", &context.workspace),
                ("User", &context.user),
                ("Agent", cli.agent.label()),
                (
                    "Open in",
                    if handoff {
                        "New Herdr terminal"
                    } else {
                        "This terminal"
                    },
                ),
            ],
        );
    }
    configuration::prepare_env(&mut context)?;
    let executable = install::ensure(&cli, &context)?;
    if handoff {
        let mut child = cli.clone();
        child.install = false;
        child.no_input = true;
        if std::path::Path::new(&executable).is_absolute() {
            child.agent_path = Some(executable.clone());
        }
        if herdr::launch(&child, &context, &tail)? {
            return Ok(());
        }
    }
    launch::run(&cli, &context, &tail, &executable)
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
