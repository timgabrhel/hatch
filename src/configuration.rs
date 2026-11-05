//! Delegate JSONC, feature metadata and variable substitution to the official CLI.
use crate::{
    cli::Cli,
    discovery::{self, Container, Context},
    process::capture,
    Result,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) fn local_docker() -> bool {
    if !std::env::var("DOCKER_CONTEXT").is_ok_and(|value| !value.is_empty()) {
        if let Ok(host) = std::env::var("DOCKER_HOST") {
            if !host.is_empty() {
                return host.starts_with("unix://");
            }
        }
    }
    capture(
        Command::new("docker").args([
            "context",
            "inspect",
            "--format",
            "{{.Endpoints.docker.Host}}",
        ]),
        "inspect Docker endpoint",
    )
    .map(|b| String::from_utf8_lossy(&b).trim().starts_with("unix://"))
    .unwrap_or(false)
}

fn fallback(cli: &Cli, c: &Container, interactive: bool, reason: &str) -> Result<Context> {
    if cli.require_config || cli.config.is_some() {
        return Err(reason.into());
    }
    let mut context = discovery::resolve(
        c,
        cli.workspace.as_deref(),
        cli.user.as_deref(),
        interactive,
    )?;
    if !cli.metadata_only {
        context.warnings.push(reason.into());
    }
    Ok(context)
}

pub fn resolve(cli: &Cli, c: &Container, interactive: bool) -> Result<Context> {
    if cli.metadata_only {
        return fallback(cli, c, interactive, "metadata-only mode");
    }
    let explicit = cli.config.is_some();
    let mut config = cli
        .config
        .clone()
        .or_else(|| c.label("devcontainer.config_file").map(PathBuf::from));
    if config.is_none() {
        if let Some(folder) = c
            .local_folder()
            .filter(|p| Path::new(p).is_absolute() && Path::new(p).is_dir())
        {
            if local_docker() {
                let root = Path::new(folder);
                config = [
                    root.join(".devcontainer/devcontainer.json"),
                    root.join(".devcontainer.json"),
                ]
                .into_iter()
                .find(|p| p.is_file());
                if config.is_none() {
                    let mut profiles = std::fs::read_dir(root.join(".devcontainer"))
                        .into_iter()
                        .flatten()
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.path().join("devcontainer.json"))
                        .filter(|p| p.is_file())
                        .collect::<Vec<_>>();
                    profiles.sort();
                    if profiles.len() == 1 {
                        config = profiles.pop();
                    } else if profiles.len() > 1 {
                        if !interactive {
                            return Err("Multiple devcontainer profiles found; supply --config PATH to select the running container's profile".into());
                        }
                        let labels = profiles
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>();
                        let selected = crate::prompt::choose(
                            "Select devcontainer configuration",
                            &labels,
                            true,
                        )?;
                        config = Some(profiles.remove(selected));
                    }
                }
            }
        }
    }
    let Some(config) = config else {
        return fallback(
            cli,
            c,
            interactive,
            "No devcontainer config label; use --config and --host-workspace for full resolution.",
        );
    };
    if !config.is_file() {
        return fallback(cli, c, interactive, "Devcontainer config is not available locally; use --config and --host-workspace, or explicit workspace/user overrides.");
    }
    if !explicit && !local_docker() {
        return fallback(cli, c, interactive, "Docker is remote; host config paths are not assumed local. Supply --config and --host-workspace explicitly.");
    }
    let config = config.canonicalize().map_err(|e| e.to_string())?;
    let folder = if let Some(folder) = &cli.host_workspace {
        Some(
            folder
                .canonicalize()
                .map_err(|e| format!("host workspace: {e}"))?,
        )
    } else if !explicit || local_docker() {
        c.local_folder()
            .filter(|p| Path::new(p).is_absolute() && Path::new(p).is_dir())
            .map(PathBuf::from)
    } else {
        None
    };
    // Avoid guessing the root for nested multi-profile configurations.
    if folder.is_none() {
        return fallback(
            cli,
            c,
            interactive,
            "Cannot establish local workspace for config substitution; supply --host-workspace.",
        );
    }
    let mut command = Command::new(&cli.devcontainer_cli);
    command
        .args([
            "read-configuration",
            "--container-id",
            &c.id,
            "--include-merged-configuration",
            "--log-format",
            "json",
        ])
        .arg("--config")
        .arg(&config)
        .arg("--workspace-folder")
        .arg(folder.as_ref().unwrap());
    // Resolver logs can contain remoteEnv secrets. Never print raw output on failure.
    let output = match command.stdin(Stdio::null()).output() {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return fallback(cli, c, interactive,
            "Dev Container CLI unavailable; install @devcontainers/cli or use --devcontainer-cli PATH. Resolving from Docker metadata only."),
        Err(e) => return Err(format!("could not run Dev Container resolver: {e}")),
    };
    if !output.status.success() {
        return Err(format!("Dev Container resolver failed ({}). Check {:?} and its referenced files; raw logs are withheld because they may include environment secrets. Use --metadata-only to explicitly bypass the resolver.", output.status, config));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Invalid JSON from Dev Container resolver (output withheld)")?;
    let mut context = merge(cli, c, &value, interactive)?;
    context.config_file = Some(config);
    context.host_workspace = folder;
    Ok(context)
}

pub fn merge(cli: &Cli, c: &Container, value: &Value, interactive: bool) -> Result<Context> {
    let merged = value
        .get("mergedConfiguration")
        .filter(|v| v.is_object())
        .ok_or("Resolver omitted mergedConfiguration")?;
    let string = |key: &str| -> Result<Option<&str>> {
        match merged.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            _ => Err(format!("Invalid {key} in resolved configuration")),
        }
    };
    if let Some(service) = string("service")? {
        if c.label("com.docker.compose.service") != Some(service) {
            return Err(
                "Selected container does not match the devcontainer Compose service".into(),
            );
        }
    }
    let workspace = cli
        .workspace
        .as_deref()
        .or(string("workspaceFolder")?)
        .or_else(|| {
            value
                .pointer("/workspace/workspaceFolder")
                .and_then(Value::as_str)
        });
    let user = cli.user.as_deref().or(string("remoteUser")?);
    let mut context = discovery::resolve(c, workspace, user, interactive)?;
    context.configuration_source = "devcontainer-cli".into();
    if cli.workspace.is_none() && workspace.is_some() {
        context.workspace_source = "resolved devcontainer workspaceFolder".into();
    }
    if cli.user.is_none() && user.is_some() {
        context.user_source = "resolved devcontainer remoteUser".into();
    }
    context.user_env_probe = Some(
        string("userEnvProbe")?
            .unwrap_or("loginInteractiveShell")
            .into(),
    );
    if ![
        "none",
        "loginShell",
        "interactiveShell",
        "loginInteractiveShell",
    ]
    .contains(&context.user_env_probe.as_deref().unwrap())
    {
        return Err("Unsupported userEnvProbe mode".into());
    }
    if let Some(env) = merged.get("remoteEnv") {
        let env = env.as_object().ok_or("remoteEnv must be an object")?;
        for (key, value) in env {
            if key.is_empty() || key.contains(['=', '\0']) {
                return Err("Invalid remoteEnv variable name".into());
            }
            let value = match value {
                Value::Null => None,
                Value::String(s) if !s.contains("${") && !s.contains('\0') => Some(s.clone()),
                _ => {
                    return Err(format!(
                        "Invalid or unresolved remoteEnv value for {key}; value withheld"
                    ))
                }
            };
            context.remote_env.insert(key.clone(), value);
        }
    }
    context.remote_env_names = context.remote_env.keys().cloned().collect();
    Ok(context)
}

/// Only launch performs configured shell probing; inspect never runs startup scripts.
pub fn prepare_env(context: &mut Context) -> Result<()> {
    let flags = match context.user_env_probe.as_deref() {
        None | Some("none") => return Ok(()),
        Some("loginShell") => "-lc",
        Some("interactiveShell") => "-ic",
        Some("loginInteractiveShell") => "-lic",
        _ => return Err("Unsupported userEnvProbe mode".into()),
    };
    let mut clean = context.clone();
    clean.remote_env.clear();
    let shell_probe = r#"set -eu
uid=$(id -u)
while IFS=: read -r name password entry_uid gid comment home shell; do
    if [ "$entry_uid" = "$uid" ]; then printf '%s\000%s' "$home" "$shell"; exit 0; fi
done < /etc/passwd
exit 1"#;
    let output = crate::launch::script(&clean, shell_probe, "hatch-shell", &[])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("shell discovery failed: {e}"))?;
    if !output.status.success() {
        return Err("Cannot detect container user's shell for userEnvProbe; set userEnvProbe to none or use --metadata-only.".into());
    }
    let identity = String::from_utf8(output.stdout).map_err(|_| "Invalid shell identity")?;
    let (home, shell) = identity
        .split_once('\0')
        .ok_or("Invalid passwd identity response")?;
    discovery::absolute_path(home)?;
    discovery::absolute_path(shell)?;
    clean.remote_env.insert("HOME".into(), Some(home.into()));
    clean.remote_env.insert("SHELL".into(), Some(shell.into()));
    let name = Path::new(&shell)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("");
    if !["sh", "bash", "dash", "ash", "zsh", "ksh"].contains(&name) {
        return Err(format!("Shell {name:?} is not supported for environment probing; use remoteEnv and userEnvProbe:none."));
    }
    let output = crate::launch::base(&clean)
        .arg(&context.container_id)
        .arg(shell)
        .args([flags, "printf '\\000HATCH_ENV\\000'; env -0"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("environment probe failed: {e}"))?;
    if !output.status.success() {
        return Err("userEnvProbe failed; shell output withheld because it may contain secrets. Set userEnvProbe:none to bypass.".into());
    }
    let sentinel = b"\0HATCH_ENV\0";
    let start = output
        .stdout
        .windows(sentinel.len())
        .position(|v| v == sentinel)
        .ok_or("environment probe marker missing")?
        + sentinel.len();
    let mut env = std::collections::BTreeMap::new();
    for pair in output.stdout[start..]
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
    {
        let pair =
            std::str::from_utf8(pair).map_err(|_| "Non-UTF-8 shell environment is unsupported")?;
        let (key, value) = pair
            .split_once('=')
            .ok_or("Invalid environment probe result")?;
        if !["PWD", "OLDPWD", "SHLVL", "_"].contains(&key) {
            env.insert(key.to_owned(), Some(value.to_owned()));
        }
    }
    env.extend(context.remote_env.clone()); // Explicit remoteEnv wins over shell values.
    context.remote_env = env;
    context.remote_env_names = context.remote_env.keys().cloned().collect();
    Ok(())
}
