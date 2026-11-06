use crate::{cli::Cli, Result};
use std::{ffi::OsString, process::Command};

pub fn run(cli: &Cli, tail: &[OsString]) -> Result<()> {
    if !tail.is_empty()
        || cli.container.is_some()
        || cli.config.is_some()
        || cli.host_workspace.is_some()
        || cli.workspace.is_some()
        || cli.user.is_some()
        || cli.agent_explicit
        || cli.agent_path.is_some()
        || cli.install
        || cli.worktree.is_some()
        || cli.setup_command.is_some()
        || cli.init_skill.is_some()
        || cli.herdr
        || cli.herdr_workspace.is_some()
        || cli.metadata_only
        || cli.require_config
    {
        return Err(
            "hatch update updates the host executable; container and agent options do not apply."
                .into(),
        );
    }
    let executable =
        std::env::current_exe().map_err(|e| format!("locate Hatch executable: {e}"))?;
    if executable.file_name().is_none_or(|name| name != "hatch") {
        return Err("The running executable must be named hatch to update it in place.".into());
    }
    let directory = executable
        .parent()
        .ok_or("Hatch has no install directory")?;
    // Use the installer shipped in this executable, never a script fetched from main.
    // Updates preserve the current installation location and never launch an agent.
    let status = Command::new("sh")
        .args(["-c", include_str!("../scripts/install.sh"), "hatch-update"])
        .args(["--update", "--install-dir"])
        .arg(directory)
        .status()
        .map_err(|e| format!("start Hatch updater: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Hatch update failed. See the installer error above; check release access and installation-directory permissions.".into())
    }
}
