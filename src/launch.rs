use crate::{
    cli::{Agent, Cli},
    discovery::{absolute_path, Context},
    process::capture,
    Result,
};
use std::{
    ffi::OsString,
    io::{self, IsTerminal},
    os::unix::process::CommandExt,
    process::Command,
};

pub fn executable(cli: &Cli) -> Result<&str> {
    if let Some(path) = cli.agent_path.as_deref() {
        absolute_path(path)?;
        Ok(path)
    } else {
        Ok(cli.agent.executable())
    }
}

pub(crate) fn base(context: &Context) -> Command {
    let mut command = Command::new("docker");
    command.args([
        "exec",
        "--user",
        &context.user,
        "--workdir",
        &context.workspace,
    ]);
    for (key, value) in &context.remote_env {
        if let Some(value) = value {
            command.arg("--env").arg(format!("{key}={value}"));
        }
    }
    command
}

/// Append the container-side process. env replaces itself, so no wrapper remains.
pub(crate) fn target<'a>(
    command: &'a mut Command,
    context: &Context,
    executable: &str,
) -> &'a mut Command {
    command.arg(&context.container_id);
    let unset: Vec<_> = context
        .remote_env
        .iter()
        .filter(|(_, v)| v.is_none())
        .collect();
    if !unset.is_empty() {
        command.arg("env");
        for (key, _) in unset {
            command.arg("-u").arg(key);
        }
    }
    command.arg(executable)
}
pub(crate) fn script(context: &Context, script: &str, marker: &str, args: &[&str]) -> Command {
    let mut command = base(context);
    target(&mut command, context, "sh")
        .args(["-c", script, marker])
        .args(args);
    command
}

pub fn probe(context: &Context, executable: &str, agent: Agent) -> Result<()> {
    // No shell or package manager is required. Running --version also diagnoses
    // missing dynamic loaders, bad user/workdir, stopped containers and runtimes.
    // Failure is not interpreted as permission to install or upgrade anything.
    capture(target(&mut base(context), context, executable).arg("--version"), "agent preflight")
        .map(|_| ())
        .map_err(|e| format!("{e}\nCannot run {} as {:?} in {:?}. Check the directory, user, and container-local installation/runtime. If it is installed outside Docker's PATH, supply --agent-path /absolute/path/to/{}. Use --install to install a missing CLI inside the container as the intended user. No host credentials or state are needed.", agent.executable(), context.user, context.container_name, agent.executable()))
}

pub fn command(
    context: &Context,
    agent: Agent,
    executable: &str,
    tail: &[OsString],
    tty: bool,
) -> Command {
    let mut command = base(context);
    // Always retain stdin, including pipes. A TTY is allocated only when both
    // stdin and stdout are terminals. --no-input controls Hatch prompts only.
    command.arg("-i");
    if tty {
        command.arg("-t");
    }
    if tty {
        for key in ["TERM", "COLORTERM"] {
            if let Some(value) = std::env::var_os(key) {
                let mut assignment = OsString::from(key);
                assignment.push("=");
                assignment.push(value);
                command.arg("--env").arg(assignment);
            }
        }
    }
    target(&mut command, context, executable).args(tail);
    // This is host-side identification for terminal managers, not credential
    // forwarding. No agent flags are injected into login/resume/exec commands.
    command.env("HERDR_AGENT", agent.executable());
    command
}

pub fn run(cli: &Cli, context: &Context, tail: &[OsString], executable: &str) -> Result<()> {
    let tty = !cli.no_tty && io::stdin().is_terminal() && io::stdout().is_terminal();
    let error = command(context, cli.agent, executable, tail, tty).exec();
    Err(format!("could not replace Hatch with docker exec: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn exact_command_and_non_utf8_arguments() {
        let context = Context {
            configuration_source: "test".into(),
            config_file: None,
            host_workspace: None,
            user_env_probe: None,
            warnings: Vec::new(),
            remote_env_names: Vec::new(),
            remote_env: Default::default(),
            container_id: "id".into(),
            container_name: "name".into(),
            user: "dev".into(),
            user_source: "test".into(),
            workspace: "/work space".into(),
            workspace_source: "test".into(),
            daemon_host_path: None,
        };
        let tail = vec![
            "login".into(),
            "".into(),
            "a b'\";$HOME\n".into(),
            OsString::from_vec(vec![255]),
        ];
        let cmd = command(&context, Agent::Codex, "codex", &tail, false);
        let args: Vec<_> = cmd.get_args().map(|v| v.to_owned()).collect();
        let mut expected: Vec<OsString> = [
            "exec",
            "--user",
            "dev",
            "--workdir",
            "/work space",
            "-i",
            "id",
            "codex",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        expected.extend(tail);
        assert_eq!(args, expected);
        assert_eq!(
            cmd.get_envs().collect::<Vec<_>>(),
            vec![(
                std::ffi::OsStr::new("HERDR_AGENT"),
                Some(std::ffi::OsStr::new("codex"))
            )]
        );
        assert!(command(&context, Agent::Claude, "claude", &[], true)
            .get_args()
            .any(|v| v == "-t"));
    }
}
