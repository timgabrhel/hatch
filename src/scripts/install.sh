# Runs entirely inside the container as the selected development user.
set -eu
agent=$1
mode=$2
case "$agent" in
    codex) interpreter=sh; url=https://chatgpt.com/codex/install.sh ;;
    claude) interpreter=bash; url=https://claude.ai/install.sh ;;
    copilot) interpreter=bash; url=https://gh.io/copilot-install ;;
    opencode) interpreter=bash; url=https://opencode.ai/v2/install ;;
    cursor-agent) interpreter=bash; url=https://cursor.com/install ;;
    grok) interpreter=bash; url=https://x.ai/cli/install.sh ;;
    *) echo 'Unsupported agent' >&2; exit 1 ;;
esac
extra_tools=
case "$agent" in
    copilot) extra_tools="cp id" ;;
    opencode) extra_tools="cp cat sort wc ls sleep" ;;
    cursor-agent) extra_tools="cp cat" ;;
    grok) extra_tools="cp cat readlink od sort wc" ;;
esac
for tool in $extra_tools "$interpreter" curl mktemp rm mkdir chmod uname tar gzip awk sed grep head cut tr ln mv date basename dirname sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || { echo "Installation requires $tool inside the container; provision it or install the agent manually." >&2; exit 1; }
done
case "$(uname -s):$(uname -m)" in
    Linux:x86_64|Linux:aarch64|Linux:arm64) ;;
    *) echo 'Hatch installers currently support Linux x86_64 and ARM64 containers only.' >&2; exit 1 ;;
esac
case "${HOME:-}" in /*) ;; *) echo 'Installation requires an absolute container HOME.' >&2; exit 1;; esac
case "$agent" in
    cursor-agent) storage="$HOME/.local/share/cursor-agent" ;;
    *) storage="$HOME/.$agent" ;;
esac
for location in "$HOME/.local/bin" "$storage"; do
    while [ ! -e "$location" ]; do
        location=${location%/*}
        [ -n "$location" ] || location=/
    done
    [ -d "$location" ] && [ -w "$location" ] || { echo "Installation directory is not writable: $location" >&2; exit 1; }
done
[ "$mode" = install ] || exit 0
# Fetch completely before execution; a partial download is never executed.
installer=$(mktemp)
trap 'rm -f "$installer"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --connect-timeout 30 --max-time 180 "$url" -o "$installer"
# Keep the native Codex installer noninteractive and avoid profile edits solely
# to add its bin directory. Never copy host environment/credentials into Docker.
PATH="$HOME/.local/bin:$HOME/.opencode/bin:$HOME/.grok/bin:$PATH"
export PATH
case "$agent" in
    codex) CODEX_NON_INTERACTIVE=1 CODEX_INSTALL_DIR="$HOME/.local/bin" sh "$installer" ;;
    copilot) PREFIX="$HOME/.local" VERSION=latest bash "$installer" ;;
    opencode) VERSION= bash "$installer" --no-modify-path ;;
    # Include .grok/bin on PATH to avoid replacing Cursor's generic agent alias.
    # The installer uses SHELL only for optional completion/profile setup.
    grok) GROK_BIN_DIR="$HOME/.grok/bin" SHELL=/bin/sh bash "$installer" ;;
    *) bash "$installer" ;;
esac
