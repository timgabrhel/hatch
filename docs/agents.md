# Agents, installation, and authentication

[← Back to README](../README.md)

Hatch installs only the selected missing agent, inside the selected container as its
development user. It does not copy host credentials or edit devcontainer configuration.

## Discovery

Hatch first tries `AGENT --version`. If that fails, a detected POSIX shell checks PATH,
`~/.local/bin`, `~/.npm-global/bin`, `~/.opencode/bin`, and `~/.grok/bin`.

A present but broken CLI is not overwritten.
`--agent-path /absolute/container/path` selects a custom executable and disables
fallback installation.

## Installation and consent

The final review shows **Installed** for a working executable, or yellow **! Install**
when installation is needed. Choose **Install Agent & Launch** to approve installation
and launch together. Escape returns to the preceding choice; there is no separate install prompt.

No agent is installed before that final confirmation. Failed prerequisites or
installation prevent launch and Herdr terminal creation.

Unattended installation requires explicit opt-in:

```sh
hatch --container my-devcontainer --install --no-input -- --version
```

## Installer requirements

All native installers run **inside the container as the selected user**. Hatch checks
Linux x86_64/ARM64, required tools, and write permissions. POSIX sh, curl, and common
Unix utilities are needed; every installer except Codex additionally needs Bash.
Agent-specific tools are checked (for example readlink for Grok). Neither root
nor a package manager is assumed.

Native installers may configure container-local shell
files; Hatch does not change host profiles or transfer host credentials.

A complete HTTPS download precedes execution. Temporary scripts are cleaned up,
and the installed user-local binary is verified before launch.

Hatch installs missing CLIs; it does not upgrade valid installations. Native agents
may have their own update behavior.

## Authentication and inspection

Use the agent itself to log in or verify access. Provider subscriptions, API keys,
and login state remain agent-specific and container-local.

`hatch inspect` checks all supported executables and reports their availability and
paths. It uses the configured launch environment, including shell probing, but does
not check authentication, invoke a model, or change sandbox settings.

Configure persistence in your devcontainer if needed. Hatch does not add volumes,
copy credentials, or modify your config.

## Supported agents

Agent keys, executables, and vendor installation documentation:

| `--agent` | Executable | Installer |
|---|---|---|
| `codex` (default) | `codex` | [Codex](https://learn.chatgpt.com/docs/codex/cli) |
| `claude` | `claude` | [Claude](https://code.claude.com/docs/en/setup) |
| `copilot` | `copilot` | [GitHub Copilot](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli) |
| `opencode` | `opencode` | [OpenCode 2](https://dev.opencode.ai/v2/docs/) |
| `cursor` | `cursor-agent` | [Cursor](https://cursor.com/docs/cli/installation) |
| `grok` | `grok` | [Grok Build](https://docs.x.ai/build/overview) |

OpenCode installs V2 when missing and leaves working existing versions intact. Cursor
and Grok use distinct commands instead of their shared `agent` alias.

Agents may manage their own background processes and updates; Hatch does not manage those services.
