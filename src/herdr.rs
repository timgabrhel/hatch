//! Optional terminal handoff. No commands are ever sent to an existing pane.
use crate::{cli::Cli, discovery::Context, process::capture, prompt, Result};
use serde_json::Value;
use std::{ffi::OsString, process::Command};

pub fn inside_core() -> bool {
    std::env::var("HC_TERMINAL_ID").is_ok_and(|id| !id.trim().is_empty())
}

pub fn inside() -> bool {
    inside_core() || std::env::var("HERDR_ENV").as_deref() == Ok("1")
}

/// Read-only capabilities resolved before review, then used for the actual handoff.
#[derive(Clone)]
pub enum Destination {
    Core {
        executable: String,
        cwd: std::path::PathBuf,
    },
    Legacy {
        cwd: std::path::PathBuf,
    },
}

fn core_executable() -> Option<String> {
    ["hc", "herdr-core"]
        .into_iter()
        .find(|exe| {
            Command::new(exe)
                .arg("--version")
                .output()
                .is_ok_and(|out| {
                    out.status.success() && String::from_utf8_lossy(&out.stdout).starts_with("hc ")
                })
        })
        .map(str::to_owned)
}

fn verify_core(executable: &str) -> Result<()> {
    let help = capture(
        Command::new(executable).args(["run", "--help"]),
        "check Herdr Core launch capabilities",
    )?;
    let help = String::from_utf8_lossy(&help);
    for flag in [
        "--json",
        "--bg",
        "--no-parent",
        "--no-notify",
        "--cwd",
        "--label",
    ] {
        if !help.contains(flag) {
            return Err(format!("Herdr Core does not support {flag}"));
        }
    }
    let terminal = std::env::var("HC_TERMINAL_ID").map_err(|e| e.to_string())?;
    let current = capture(
        Command::new(executable).args(["get", &terminal, "--json"]),
        "verify current Herdr Core terminal",
    )?;
    let current: Value =
        serde_json::from_slice(&current).map_err(|_| "Invalid Herdr Core terminal response")?;
    if current.get("address").and_then(Value::as_str) != Some(&terminal)
        || current.get("state").and_then(Value::as_str) != Some("live")
    {
        return Err("Herdr Core did not identify this live terminal".into());
    }
    Ok(())
}

pub fn prepare_destination(cli: &mut Cli, context: &Context) -> Result<Option<Destination>> {
    if inside_core() && cli.herdr_workspace.is_some() {
        return Err("Herdr Core does not expose desktop workspace placement over SSH. Use --herdr for a new Core terminal or --here for this terminal.".into());
    }
    let explicit = cli.herdr || cli.herdr_workspace.is_some();
    if cli.here || !explicit {
        return Ok(None);
    }
    let available = (|| -> Result<Destination> {
        let cwd = context
            .daemon_host_path
            .as_ref()
            .filter(|p| p.is_dir())
            .ok_or("No verified local project directory for Herdr")?
            .clone();
        if !crate::configuration::local_docker() {
            return Err("Herdr requires a local Docker endpoint".into());
        }
        if inside_core() {
            let executable = core_executable().ok_or("Herdr Core CLI is unavailable")?;
            verify_core(&executable)?;
            Ok(Destination::Core { executable, cwd })
        } else {
            if !inside() {
                return Err("Not inside Herdr".into());
            }
            let list = call(&["workspace", "list"])?;
            let workspaces = list
                .pointer("/result/workspaces")
                .and_then(Value::as_array)
                .ok_or("Herdr response omitted workspaces")?;
            let exists = |id: &str| {
                workspaces
                    .iter()
                    .any(|w| w.get("workspace_id").and_then(Value::as_str) == Some(id))
            };
            if let Some(id) = &cli.herdr_workspace {
                if !exists(id) {
                    return Err(format!(
                        "Herdr workspace {id:?} does not exist in this session"
                    ));
                }
            }
            Ok(Destination::Legacy { cwd })
        }
    })();
    let destination = match available {
        Ok(destination) => destination,
        Err(e) if cli.herdr_workspace.is_some() => return Err(e),
        Err(e) => {
            prompt::warning(&format!("{e}; launching in this terminal."));
            cli.herdr = false;
            cli.here = true;
            return Ok(None);
        }
    };
    Ok(Some(destination))
}

pub fn destination(cli: &Cli) -> &'static str {
    if cli.herdr_workspace.is_some() {
        "New tab in selected workspace"
    } else if cli.herdr && inside_core() {
        "New Core session (open here)"
    } else if cli.herdr {
        "New workspace"
    } else {
        "This terminal"
    }
}

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
    Ok(handoff_args(cli, context, tail)?
        .iter()
        .map(|s| quote(s))
        .collect::<Vec<_>>()
        .join(" "))
}

pub fn preview(cli: &Cli, context: &Context, tail: &[OsString]) -> Result<String> {
    let mut args = vec!["hatch".into()];
    push(&mut args, "--container", &context.container_name)?;
    if let Some(path) = &cli.worktree {
        push(&mut args, "--worktree", path)?;
    } else {
        push(&mut args, "--workspace", &context.workspace)?;
    }
    if let Some(user) = &cli.user {
        push(&mut args, "--user", user)?;
    }
    push(&mut args, "--agent", cli.agent.key())?;
    if let Some(path) = &cli.agent_path {
        push(&mut args, "--agent-path", path)?;
    }
    if let Some(id) = &cli.herdr_workspace {
        push(&mut args, "--herdr-workspace", id)?;
    } else if cli.herdr {
        args.push("--herdr".into());
    } else {
        args.push("--here".into());
    }
    if cli.install {
        args.push("--install".into());
    }
    for (key, value) in [
        ("--setup-command", cli.setup_command.as_ref()),
        ("--init-skill", cli.init_skill.as_ref()),
    ] {
        if let Some(value) = value {
            push(&mut args, key, value)?;
        }
    }
    if let Some(path) = &cli.config {
        push(&mut args, "--config", prompt::local_path(path))?;
        if let Some(root) = &cli.host_workspace {
            push(&mut args, "--host-workspace", prompt::local_path(root))?;
        }
    }
    if cli.devcontainer_cli != "devcontainer" {
        push(&mut args, "--devcontainer-cli", &cli.devcontainer_cli)?;
    }
    for (enabled, flag) in [
        (cli.metadata_only, "--metadata-only"),
        (cli.require_config, "--require-config"),
        (cli.no_tty, "--no-tty"),
        (cli.plain, "--plain"),
    ] {
        if enabled {
            args.push(flag.into());
        }
    }
    args.push("--no-input".into());
    if !tail.is_empty() {
        args.push("--".into());
        for arg in tail {
            let Some(arg) = arg.to_str() else {
                return Ok(
                    "Command contains non-UTF-8 arguments; cannot display it for copying.".into(),
                );
            };
            args.push(arg.into());
        }
    }
    if args.iter().any(|arg| arg.chars().any(char::is_control)) {
        return Ok("Command contains control characters; cannot display it for copying.".into());
    }
    Ok(copyable_command(
        &args,
        usize::from(console::Term::stderr().size().1).max(16) - 1,
    ))
}

// Keep physical rows bounded for dialoguer's redraw accounting. Adjacent quoted
// chunks remain one shell word, even across an escaped newline.
fn copyable_command(args: &[String], width: usize) -> String {
    let mut result = String::from("  ");
    let mut column = 2;
    let mut expects_value = false;
    let mut agent_arguments = false;
    for (i, arg) in args.iter().enumerate() {
        let option = i > 0 && !agent_arguments && !expects_value && arg.starts_with("--");
        let safe = !arg.is_empty()
            && arg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_./:@%+=,-".contains(&b));
        let rendered_width = if safe {
            arg.len()
        } else {
            console::measure_text_width(&quote(arg))
        };
        if i > 0 {
            if option || agent_arguments || column + rendered_width + 3 > width {
                result.push_str(" \\\n    ");
                column = 4;
            } else {
                result.push(' ');
                column += 1;
            }
        }
        if option {
            agent_arguments = arg == "--";
            expects_value = !matches!(
                arg.as_str(),
                "--" | "--here"
                    | "--herdr"
                    | "--no-input"
                    | "--install"
                    | "--metadata-only"
                    | "--require-config"
                    | "--no-tty"
                    | "--plain"
            );
        } else {
            expects_value = false;
        }
        if safe && column + rendered_width + 2 <= width {
            result.push_str(arg);
            column += rendered_width;
            continue;
        }
        result.push('\'');
        column += 1;
        for ch in arg.chars() {
            let encoded = if ch == '\'' {
                "'\"'\"'".to_owned()
            } else {
                ch.to_string()
            };
            let size = console::measure_text_width(&encoded);
            if column + size + 3 > width {
                result.push_str("'\\\n'");
                column = 1;
            }
            result.push_str(&encoded);
            column += size;
        }
        result.push('\'');
        column += 1;
    }
    result
}

fn handoff_args(cli: &Cli, context: &Context, tail: &[OsString]) -> Result<Vec<String>> {
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
    // A persistent terminal server may have an older PATH than the launching shell.
    // Pin host tool discovery, including node used by the Dev Container CLI's shebang.
    // Resolve relative entries now because the new terminal can have a different cwd.
    if let Some(path) = std::env::var_os("PATH") {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let path = std::env::join_paths(std::env::split_paths(&path).map(|entry| {
            if entry.is_absolute() {
                entry
            } else {
                cwd.join(entry)
            }
        }))
        .map_err(|e| format!("Host PATH: {e}"))?;
        args.push(format!(
            "PATH={}",
            path.to_str().ok_or("Host PATH is not UTF-8")?
        ));
    } else {
        // Match the caller's unset PATH rather than retaining the server's value.
        // env's -u options must precede assignments, so insert this at the front.
        args.splice(1..1, ["-u".into(), "PATH".into()]);
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
    let resolver = std::path::Path::new(&cli.devcontainer_cli);
    if resolver.is_relative() && resolver.components().count() > 1 {
        let resolver = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(resolver);
        push(&mut args, "--devcontainer-cli", resolver)?;
    } else {
        push(&mut args, "--devcontainer-cli", &cli.devcontainer_cli)?;
    }
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
    Ok(args)
}

pub fn launch(
    destination: &Destination,
    cli: &Cli,
    context: &Context,
    tail: &[OsString],
) -> Result<bool> {
    let cwd = match destination {
        Destination::Core { executable, cwd } => {
            return launch_core(executable, cwd, cli, context, tail)
        }
        Destination::Legacy { cwd } => cwd,
    };
    let list = call(&["workspace", "list"])?;
    let cwd = cwd.to_str().ok_or("Herdr workspace path is not UTF-8")?;
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
        let response = call(&[
            "workspace",
            "create",
            "--cwd",
            cwd,
            "--label",
            &context.container_name,
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

fn launch_core(
    executable: &str,
    cwd: &std::path::Path,
    cli: &Cli,
    context: &Context,
    tail: &[OsString],
) -> Result<bool> {
    // The selected backend is pinned before review; failure never switches it.
    verify_core(executable)?;
    let args = handoff_args(cli, context, tail)?;
    let bytes = capture(
        Command::new(executable)
            .args([
                "run",
                "--json",
                "--bg",
                "--no-parent",
                "--no-notify",
                "--cwd",
            ])
            .arg(cwd)
            .args([
                "--label",
                &format!("hatch.container={}", context.container_name),
                "--",
            ])
            .args(args),
        "create Herdr Core terminal",
    )
    .map_err(|e| {
        format!("{e}. A terminal may have been created; inspect hc ls before retrying.")
    })?;
    let result: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Invalid Herdr Core launch response; inspect hc ls before retrying")?;
    let address = result
        .get("address")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or("Herdr Core omitted the terminal address; inspect hc ls before retrying")?;
    if result.get("state").and_then(Value::as_str) != Some("live") {
        return Err(format!(
            "Herdr Core terminal {address} did not remain live; inspect hc get {address}."
        ));
    }
    eprintln!(
        "Started {} in Herdr Core terminal {address} (detached).",
        cli.agent.label()
    );
    prompt::note(&format!("Open this session: {executable} attach {address}"));
    use std::{io::IsTerminal, os::unix::process::CommandExt};
    if !cli.no_tty && std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        let error = Command::new(executable).args(["attach", address]).exec();
        return Err(format!("Could not attach to Core session {address}: {error}. It is still available with {executable} attach {address}"));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copyable_command_preserves_arguments_on_narrow_terminals() {
        let args = vec![
            "printf".into(),
            "%s\\0".into(),
            "".into(),
            "a b'\";$(touch nope)".into(),
            "very-long-path/".repeat(12),
            "日本語".repeat(10),
        ];
        for width in [15, 39, 79] {
            let command = copyable_command(&args, width);
            assert!(command
                .lines()
                .all(|line| console::measure_text_width(line) <= width));
            let output = Command::new("sh").args(["-c", &command]).output().unwrap();
            assert!(output.status.success());
            let expected = args[2..]
                .iter()
                .flat_map(|arg| arg.bytes().chain([0]))
                .collect::<Vec<_>>();
            assert_eq!(output.stdout, expected);
        }
    }
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
