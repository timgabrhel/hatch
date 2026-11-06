use crate::{
    cli::{Agent, Cli},
    discovery::Context,
    launch,
    process::capture,
    Result,
};
use serde_json::json;

fn check_with(
    cli: &Cli,
    context: &Context,
    mut completed: impl FnMut(&serde_json::Value),
) -> Vec<serde_json::Value> {
    // One short-lived worker per supported agent (currently six). Each worker
    // performs locate then version serially, but independent agents run together.
    // Only this thread renders output; scoped workers are joined before returning.
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        for (index, agent) in Agent::ALL.into_iter().enumerate() {
            let sender = sender.clone();
            scope.spawn(move || {
                let result = check_one(cli, context, agent);
                let _ = sender.send((index, result));
            });
        }
        drop(sender);
        let mut results = Vec::with_capacity(Agent::ALL.len());
        for (index, result) in receiver {
            completed(&result);
            results.push((index, result));
        }
        // Completion order is useful interactively; scripts get stable ordering.
        results.sort_by_key(|(index, _)| *index);
        results.into_iter().map(|(_, result)| result).collect()
    })
}

fn check_one(cli: &Cli, context: &Context, agent: Agent) -> serde_json::Value {
    let explicit = if agent == cli.agent {
        cli.agent_path.as_deref()
    } else {
        None
    };
    // Resolve even a working PATH entry so inspection has an
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
    json!({"agent":agent.key(), "available":executable.is_some(), "executable":executable})
}

pub fn scan(cli: &Cli, context: &mut Context, json_output: bool) -> Result<Vec<serde_json::Value>> {
    if !json_output {
        println!(
            "Agents in {}\n",
            crate::prompt::clean(&context.container_name)
        );
        println!(
            "Checking agents concurrently. Results appear as they finish. Ctrl-C to cancel.\n"
        );
        println!("  {:<20} {:<24} SELECT WITH", "AGENT", "CLI");
    }
    crate::configuration::prepare_env(context)?;
    let agents = check_with(cli, context, |agent| {
        if !json_output {
            let executable = agent["executable"].as_str();
            let key = agent["agent"].as_str().unwrap();
            let name = Agent::ALL.iter().find(|a| a.key() == key).unwrap().label();
            let installed = status(
                executable.is_some(),
                if executable.is_some() {
                    "Available"
                } else {
                    "Missing/unusable"
                },
            );
            println!("  {name:<20} {installed} --agent {key}");
            if let Some(path) = executable {
                println!("    {}", crate::prompt::clean(path));
            }
        }
    });
    if !json_output {
        println!("\nRun hatch to launch an agent or install a missing one.");
    }
    Ok(agents)
}

fn status(success: bool, label: &str) -> console::StyledObject<String> {
    let symbol = match (crate::prompt::plain(), success) {
        (true, true) => "[ok]",
        (true, false) => "[x]",
        (false, true) => "✓",
        (false, false) => "✗",
    };
    let text = console::style(format!("{:<24}", format!("{symbol} {label}")));
    if success {
        text.green()
    } else {
        text.red()
    }
}
