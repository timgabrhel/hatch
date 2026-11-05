use crate::{
    cli::{Agent, Cli},
    discovery::{Container, Context},
    launch,
    process::capture,
    Result,
};
use serde_json::json;

pub fn run(cli: &Cli, context: &Context, container: &Container, json_output: bool) -> Result<()> {
    let mut agents = Vec::new();
    for agent in Agent::ALL {
        let explicit = if agent == cli.agent {
            cli.agent_path.as_deref()
        } else {
            None
        };
        // Resolve even a working PATH entry so persistence diagnostics have an
        // actual container path. A shell-less image can still use direct probe.
        let candidate = explicit
            .map(str::to_owned)
            .or_else(|| {
                capture(
                    &mut launch::script(
                        context,
                        include_str!("scripts/locate.sh"),
                        "hatch-locate",
                        &[agent.executable()],
                    ),
                    "locate agent",
                )
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .and_then(|text| text.strip_prefix("found\0").map(str::to_owned))
            })
            .unwrap_or_else(|| agent.executable().into());
        let executable = launch::probe(context, &candidate, agent)
            .ok()
            .map(|_| candidate);
        let auth = if let Some(path) = executable
            .as_ref()
            .filter(|_| matches!(agent, Agent::Codex | Agent::Claude))
        {
            let args = match agent {
                Agent::Codex => vec!["login", "status"],
                Agent::Claude => vec!["auth", "status"],
                _ => unreachable!(),
            };
            // Do not expose email, token, account, or raw auth diagnostics.
            let output = launch::target(&mut launch::base(context), context, path)
                .args(args)
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| e.to_string())?;
            let authenticated = output.status.success()
                && (agent != Agent::Claude
                    || serde_json::from_slice::<serde_json::Value>(&output.stdout)
                        .ok()
                        .and_then(|v| v.get("loggedIn").and_then(|v| v.as_bool()))
                        .unwrap_or(false));
            if authenticated {
                "authenticated"
            } else {
                "not authenticated or status unavailable"
            }
        } else if executable.is_some() {
            "not checked: use the agent's own login/status command"
        } else {
            "not checked"
        };
        let persistent = executable
            .as_deref()
            .filter(|p| std::path::Path::new(p).is_absolute())
            .map(|p| {
                container.mounts.iter().any(|m| {
                    std::path::Path::new(p).starts_with(&m.destination)
                        && ["bind", "volume"].contains(&m.kind.as_str())
                })
            });
        agents.push(
            json!({"agent":agent.key(),"executable":executable,"authentication":auth,
            "executable_on_persistent_mount":persistent}),
        );
    }
    if !json_output {
        println!(
            "Agent checks — {}\n",
            crate::prompt::clean(&context.container_name)
        );
        println!("  {:<20} {:<24} LOGIN", "AGENT", "CLI");
        for agent in &agents {
            let executable = agent["executable"].as_str();
            let key = agent["agent"].as_str().unwrap();
            let name = Agent::ALL.iter().find(|a| a.key() == key).unwrap().label();
            let installed = status(
                Some(executable.is_some()),
                if executable.is_some() {
                    "Available"
                } else {
                    "Missing/unusable"
                },
            );
            let auth = agent["authentication"].as_str().unwrap();
            let login = match auth {
                "authenticated" => status(Some(true), "Authenticated"),
                "not authenticated or status unavailable" => status(None, "Not verified"),
                _ => status(None, "Not checked"),
            };
            println!("  {name:<20} {installed} {login}");
        }
        println!("\nNot verified: signed out, or authentication status is unavailable.\nNot checked: no supported auth check, or the CLI cannot run.\n\nInstall a missing agent: run hatch and choose it.\nExecutable paths and persistence: hatch doctor --json");
        return Ok(());
    }
    println!("{}", serde_json::to_string_pretty(&json!({"context":context,"agents":agents,
        "notes":["No installation, login, model request, or sandbox weakening. Configured user shell startup scripts are used to match the launch environment.",
        "Mount coverage does not prove that symlink targets, authentication, or all package files persist.",
        "Use the agent's own sandbox during real work; version/auth checks do not certify sandbox isolation."]})).map_err(|e| e.to_string())?);
    Ok(())
}

fn status(success: Option<bool>, label: &str) -> console::StyledObject<String> {
    let symbol = match (crate::prompt::plain(), success) {
        (true, Some(true)) => "[ok]",
        (true, Some(false)) => "[x]",
        (true, None) => "[?]",
        (false, Some(true)) => "✓",
        (false, Some(false)) => "✗",
        (false, None) => "?",
    };
    let text = console::style(format!("{:<24}", format!("{symbol} {label}")));
    match success {
        Some(true) => text.green(),
        Some(false) => text.red(),
        None => text.yellow(),
    }
}
