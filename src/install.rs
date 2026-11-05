use crate::{
    cli::Cli,
    discovery::{absolute_path, Context},
    launch,
    process::capture,
    prompt, Result,
};
use std::{io, process::Stdio};

pub fn ensure(cli: &Cli, context: &Context) -> Result<String> {
    let executable = launch::executable(cli)?;
    let original = match launch::probe(context, executable, cli.agent) {
        Ok(()) => return Ok(executable.into()),
        Err(error) => error,
    };
    if cli.agent_path.is_some() {
        return Err(original);
    }
    let bytes = capture(
        &mut launch::script(
            context,
            include_str!("scripts/locate.sh"),
            "hatch-locate",
            &[cli.agent.executable()],
        ),
        "locate agent (requires a working POSIX sh for fallback discovery)",
    )
    .map_err(|e| format!("{original}\n{e}"))?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let (kind, path) = text
        .split_once('\0')
        .ok_or("invalid agent discovery response")?;
    if kind == "found" {
        launch::probe(context, path, cli.agent)?;
        return Ok(path.into());
    }
    if kind != "missing" {
        return Err("invalid agent discovery status".into());
    }
    absolute_path(path)?;
    if !cli.install && !prompt::available(cli.no_input) {
        return Err(format!("Missing container-local installation.\nAgent: {}\nContainer: {}\nUser: {}\n\nRun interactively to install, or add --install.\nFor a custom executable, use --agent-path /absolute/path.", cli.agent.label(), context.container_name, context.user));
    }
    // Diagnose prerequisites before asking the user to authorize an installation.
    capture(
        &mut launch::script(
            context,
            include_str!("scripts/install.sh"),
            "hatch-install",
            &[cli.agent.executable(), "check"],
        ),
        "check installer prerequisites",
    )?;
    prompt::section(
        "Agent not installed",
        &[
            ("Agent", cli.agent.label()),
            ("Container", &context.container_name),
            ("User", &context.user),
        ],
    );
    prompt::note(
        "Installation stays inside this container. It may be lost when the container is rebuilt.",
    );
    if !cli.install && !prompt::confirm("Install now?")? {
        return Err("installation cancelled; nothing launched".into());
    }
    eprintln!(
        "Installing {} inside {:?} as {:?}...",
        cli.agent.executable(),
        context.container_name,
        context.user
    );
    let status = launch::script(
        context,
        include_str!("scripts/install.sh"),
        "hatch-install",
        &[cli.agent.executable(), "install"],
    )
    .stdin(Stdio::null())
    .stdout(Stdio::from(io::stderr()))
    .status()
    .map_err(|e| format!("could not start installer: {e}"))?;
    if !status.success() {
        return Err(format!(
            "agent installation failed ({status}); no agent launched"
        ));
    }
    let installed = format!(
        "{}/{}",
        path.trim_end_matches('/'),
        cli.agent.installed_relative()
    );
    launch::probe(context, &installed, cli.agent)
        .map_err(|e| format!("installer completed but verification failed: {e}"))?;
    Ok(installed)
}
