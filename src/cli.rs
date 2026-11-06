use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    #[default]
    Codex,
    Claude,
    Copilot,
    Opencode,
    Cursor,
    Grok,
}
impl Agent {
    pub const ALL: [Self; 6] = [
        Self::Codex,
        Self::Claude,
        Self::Copilot,
        Self::Opencode,
        Self::Cursor,
        Self::Grok,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            _ => self.executable(),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
            Self::Copilot => "GitHub Copilot",
            Self::Opencode => "OpenCode",
            Self::Cursor => "Cursor",
            Self::Grok => "Grok Build",
        }
    }

    pub fn installed_relative(self) -> &'static str {
        match self {
            Self::Opencode => ".opencode/bin/opencode",
            Self::Grok => ".grok/bin/grok",
            Self::Codex => ".local/bin/codex",
            Self::Claude => ".local/bin/claude",
            Self::Copilot => ".local/bin/copilot",
            Self::Cursor => ".local/bin/cursor-agent",
        }
    }
    pub fn executable(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Copilot => "copilot",
            Self::Opencode => "opencode",
            Self::Cursor => "cursor-agent",
            Self::Grok => "grok",
        }
    }
}

#[derive(Clone, Debug, Parser)]
#[command(
    name = "hatch",
    version,
    about = "Enter a devcontainer and launch a coding agent",
    after_help = "Start here: hatch\nInside Herdr: opens a new workspace. Use --here to stay in this terminal.\nAdvanced settings: hatch options\nAgent choices: hatch inspect\nPass agent arguments after --, e.g. hatch --agent claude -- 'Explain this repository'"
)]
pub struct Cli {
    #[arg(skip)]
    pub agent_explicit: bool,
    /// Stay in this terminal, even inside Herdr.
    #[arg(long, global = true, conflicts_with_all = ["herdr", "herdr_workspace"])]
    pub here: bool,
    /// Dev Container CLI executable (used for merged devcontainer.json resolution).
    #[arg(hide = true, long, global = true, default_value = "devcontainer")]
    pub devcontainer_cli: String,
    /// Local devcontainer.json (JSONC); selects its container unless --container is set.
    #[arg(long, global = true)]
    pub config: Option<std::path::PathBuf>,
    /// Local workspace folder or .code-workspace file for an explicit config.
    #[arg(hide = true, long, global = true, requires = "config")]
    pub host_workspace: Option<std::path::PathBuf>,
    /// Use Docker metadata only, without invoking the configuration resolver.
    #[arg(hide = true, long, global = true, conflicts_with = "config")]
    pub metadata_only: bool,
    /// Require merged configuration instead of falling back when unavailable.
    #[arg(hide = true, long, global = true, conflicts_with = "metadata_only")]
    pub require_config: bool,
    /// Launch in a new Herdr terminal; fall back to this terminal if unavailable.
    #[arg(hide = true, long, global = true)]
    pub herdr: bool,
    /// Add a new terminal to this explicit Herdr workspace ID.
    #[arg(long, global = true, hide = true)]
    pub herdr_workspace: Option<String>,
    /// Accessible line-oriented prompts instead of arrow-key menus.
    #[arg(hide = true, long, global = true)]
    pub plain: bool,
    /// Running container name or ID. Otherwise discover devcontainer labels.
    #[arg(long, global = true)]
    pub container: Option<String>,
    /// Absolute workspace directory inside the container.
    #[arg(hide = true, long, global = true)]
    pub workspace: Option<String>,
    /// Container user (name or UID, optionally with group).
    #[arg(hide = true, long, global = true)]
    pub user: Option<String>,
    /// Coding agent to run.
    #[arg(long, value_enum, default_value = "codex", global = true)]
    pub agent: Agent,
    /// Absolute in-container executable path, for agents outside Docker's PATH.
    #[arg(hide = true, long, global = true)]
    pub agent_path: Option<String>,
    /// Install the selected agent inside the container if it is missing.
    #[arg(long, global = true, conflicts_with = "agent_path")]
    pub install: bool,
    /// Do not ask questions; fail if required choices are missing (for scripts).
    #[arg(hide = true, long, global = true)]
    pub no_input: bool,
    /// Disable interactive terminal features (for scripts; normally detected automatically).
    #[arg(hide = true, long, global = true)]
    pub no_tty: bool,
    /// Select an existing Git worktree by its absolute container path.
    #[arg(long, global = true)]
    pub worktree: Option<String>,
    /// Ask Codex or Claude to read this container-local initialization skill.
    #[arg(long, global = true, conflicts_with = "setup_command")]
    pub init_skill: Option<String>,
    /// Explicit project setup shell command, run once before launching the agent.
    #[arg(long, global = true)]
    pub setup_command: Option<String>,
    #[command(subcommand)]
    pub command: Option<Action>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Action {
    /// Update this Hatch executable to the latest GitHub release.
    Update,
    /// List Git worktrees in the selected container without launching an agent.
    Worktrees {
        #[arg(long)]
        json: bool,
    },
    /// Create or remove a Git worktree in the selected container.
    Worktree {
        #[command(subcommand)]
        command: WorktreeAction,
    },
    /// Explain all settings, including advanced terminal and configuration options.
    Options,
    /// Find, start or create a devcontainer from --config, then launch the agent.
    #[command(
        after_help = "Requires --config PATH. Uses the project's normal Dev Container lifecycle hooks.\nReuses a matching running container without rebuilding it or rerunning hooks."
    )]
    Up,
    /// Browse containers interactively; --json or --no-input lists without launching.
    Containers {
        #[arg(long)]
        json: bool,
    },
    /// Inspect one container's launch context; use containers --json for the inventory.
    Inspect {
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum WorktreeAction {
    /// Create a checkout. Without --from, --branch must already exist locally.
    Create {
        /// Absolute destination inside the container on an existing persistent mount.
        path: String,
        #[arg(long)]
        branch: String,
        /// Create a new branch from this explicit local ref or commit (no fetch).
        #[arg(long = "from")]
        start: Option<String>,
    },
    /// Remove a clean, unlocked linked worktree; always retain its branch.
    Remove {
        path: String,
        /// Confirm removal, including that no session is using this checkout.
        #[arg(long)]
        yes: bool,
    },
}

/// Split before clap sees the arguments: neither clap nor a shell may interpret
/// the agent tail. OsString preserves even non-UTF-8 Unix arguments.
pub fn split_args(args: impl IntoIterator<Item = OsString>) -> (Vec<OsString>, Vec<OsString>) {
    let mut own = Vec::new();
    let mut tail = Vec::new();
    let mut forwarding = false;
    for arg in args {
        if forwarding {
            tail.push(arg);
        } else if arg == "--" {
            forwarding = true;
        } else {
            own.push(arg);
        }
    }
    (own, tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    #[test]
    fn tail_is_opaque() {
        let args = vec![
            "hatch".into(),
            "--agent".into(),
            "claude".into(),
            "--".into(),
            "--help".into(),
            "".into(),
            "$(touch /tmp/no); ' \n".into(),
            OsString::from_vec(vec![0xff, b'x']),
            "--".into(),
        ];
        let (own, tail) = split_args(args.clone());
        assert_eq!(Cli::try_parse_from(own).unwrap().agent, Agent::Claude);
        assert_eq!(tail, args[4..]);
        assert_eq!(tail[3].as_bytes(), &[0xff, b'x']);
    }

    #[test]
    fn flags_and_commands_are_unambiguous() {
        assert!(Cli::try_parse_from(["hatch", "resume"]).is_err());
        assert!(Cli::try_parse_from(["hatch", "--agent", "unknown"]).is_err());
        assert!(Cli::try_parse_from(["hatch", "--container"]).is_err());
        let c = Cli::try_parse_from(["hatch", "inspect", "--container", "dev"]).unwrap();
        assert_eq!(c.container.as_deref(), Some("dev"));
        assert_eq!(c.agent, Agent::Codex);
    }
}
