//! Optional terminal handoff. No commands are ever sent to an existing pane.
use crate::{cli::Cli, discovery::Context, process::capture, prompt, Result};
use serde_json::Value;
use std::{ffi::OsString, process::Command};

fn call(args: &[&str]) -> Result<Value> {
    let bytes = capture(Command::new("herdr").args(args), "Herdr command")?;
    // Current Herdr acknowledges pane run with exit status and no stdout;
    // topology commands return JSON. Both must be handled deliberately.
    if args.starts_with(&["pane", "run"]) && bytes.is_empty() {
        return Ok(Value::Null);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid Herdr JSON response")?;
    if value.get("error").is_some_and(|v| !v.is_null()) {
        return Err("Herdr rejected the request; inspect the workspace before retrying".into());
    }
    Ok(value)
}
fn field<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Herdr response missing {pointer}; no retry performed"))
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
fn push(args: &mut Vec<String>, key: &str, value: impl AsRef<std::ffi::OsStr>) -> Result<()> {
    let value = value.as_ref().to_str().ok_or(
        "Herdr handoff requires UTF-8 arguments; use the current terminal for arbitrary Unix bytes",
    )?;
    args.push(key.into());
    args.push(value.into());
    Ok(())
}

pub fn handoff(cli: &Cli, context: &Context, tail: &[OsString]) -> Result<String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut args = vec!["env".to_owned()];
    // Pin Docker connection inputs; don't inherit a different terminal's context.
    for key in [
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_TLS_VERIFY",
        "DOCKER_CERT_PATH",
        "DOCKER_CONFIG",
    ] {
        args.extend(["-u".into(), key.into()]);
    }
    for key in [
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_TLS_VERIFY",
        "DOCKER_CERT_PATH",
        "DOCKER_CONFIG",
    ] {
        if let Some(value) = std::env::var_os(key) {
            let value = value
                .to_str()
                .ok_or("Docker connection environment is not UTF-8")?;
            args.push(format!("{key}={value}"));
        }
    }
    if !std::env::var("DOCKER_HOST").is_ok_and(|v| !v.is_empty())
        && !std::env::var("DOCKER_CONTEXT").is_ok_and(|v| !v.is_empty())
    {
        let selected = capture(
            Command::new("docker").args(["context", "show"]),
            "resolve Docker context for Herdr",
        )?;
        let selected = String::from_utf8(selected).map_err(|e| e.to_string())?;
        args.push(format!("DOCKER_CONTEXT={}", selected.trim()));
    }
    args.push(executable.to_str().ok_or("Hatch path is not UTF-8")?.into());
    args.push("--here".into()); // Prevent automatic Herdr handoff in the child.
    push(&mut args, "--container", &context.container_id)?;
    push(&mut args, "--workspace", &context.workspace)?;
    push(&mut args, "--user", &context.user)?;
    push(&mut args, "--agent", cli.agent.key())?;
    if let Some(path) = &cli.agent_path {
        push(&mut args, "--agent-path", path)?;
    }
    if cli.install {
        args.push("--install".into());
    }
    if cli.no_input {
        args.push("--no-input".into());
    }
    if cli.no_tty {
        args.push("--no-tty".into());
    }
    if cli.plain {
        args.push("--plain".into());
    }
    if cli.metadata_only {
        args.push("--metadata-only".into());
    }
    if cli.require_config {
        args.push("--require-config".into());
    }
    push(&mut args, "--devcontainer-cli", &cli.devcontainer_cli)?;
    if let Some(path) = &context.config_file {
        push(&mut args, "--config", path)?;
    }
    if let Some(path) = &context.host_workspace {
        push(&mut args, "--host-workspace", path)?;
    }
    args.push("--".into());
    for arg in tail {
        args.push(arg.to_str().ok_or("Herdr handoff requires UTF-8 arguments; use current terminal for arbitrary Unix bytes")?.into());
    }
    Ok(args.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" "))
}

pub fn launch(cli: &Cli, context: &Context, tail: &[OsString]) -> Result<bool> {
    if std::env::var("HERDR_ENV").as_deref() != Ok("1") {
        prompt::warning("Not inside Herdr; launching in this terminal.");
        return Ok(false);
    }
    let list = match call(&["workspace", "list"]) {
        Ok(list) => list,
        Err(_) => {
            prompt::warning("Herdr is unavailable; launching in this terminal.");
            return Ok(false);
        }
    };
    // A daemon bind source is only usable as local cwd after endpoint verification.
    let cwd = context.daemon_host_path.as_ref().filter(|p| p.is_dir());
    if cwd.is_none() || !crate::configuration::local_docker() {
        prompt::warning(
            "No verified local project directory for Herdr; launching in this terminal.",
        );
        return Ok(false);
    }
    let cwd = cwd
        .unwrap()
        .to_str()
        .ok_or("Herdr workspace path is not UTF-8")?;
    let command = handoff(cli, context, tail)?; // Validate all args BEFORE creating anything.
    let workspaces = list
        .pointer("/result/workspaces")
        .and_then(Value::as_array)
        .ok_or("Herdr response omitted workspaces")?;
    let exists = |id: &str| {
        workspaces
            .iter()
            .any(|w| w.get("workspace_id").and_then(Value::as_str) == Some(id))
    };
    let workspace = if let Some(id) = &cli.herdr_workspace {
        if !exists(id) {
            return Err(format!(
                "Herdr workspace {id:?} does not exist in this session"
            ));
        }
        Some(id.clone())
    } else {
        None
    };
    let (workspace, pane) = if let Some(id) = workspace {
        let response = call(&[
            "tab",
            "create",
            "--workspace",
            &id,
            "--cwd",
            cwd,
            "--label",
            cli.agent.executable(),
            "--no-focus",
        ])?;
        let pane = field(&response, "/result/root_pane/pane_id")?.to_owned();
        (id, pane)
    } else {
        let label = format!("Hatch · {} · {}", context.container_name, cli.agent.label());
        let response = call(&[
            "workspace",
            "create",
            "--cwd",
            cwd,
            "--label",
            &label,
            "--no-focus",
        ])?;
        (
            field(&response, "/result/workspace/workspace_id")?.to_owned(),
            field(&response, "/result/root_pane/pane_id")?.to_owned(),
        )
    };
    call(&["pane","run",&pane,&command]).map_err(|e|format!("{e}. Created terminal {pane} in {workspace}; it may have received the command. Inspect it before retrying."))?;
    eprintln!(
        "Opened {} in Herdr workspace {} / pane {}.",
        cli.agent.executable(),
        workspace,
        pane
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_shell_boundaries() {
        let input = "a b'\";$(touch nope)\n";
        let out = Command::new("sh")
            .args(["-c", &format!("printf '%s' {}", quote(input))])
            .output()
            .unwrap();
        assert_eq!(out.stdout, input.as_bytes());
        assert_eq!(quote(""), "''");
    }
}
