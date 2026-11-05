# Read-only fallback after a direct executable preflight fails.
set -eu
agent=$1
if location=$(command -v "$agent"); then
    printf 'found\000%s' "$location"
    exit 0
fi
case "${HOME:-}" in /*) ;; *) echo 'Cannot resolve an absolute container HOME' >&2; exit 1;; esac
for location in "$HOME/.local/bin/$agent" "$HOME/.npm-global/bin/$agent" "$HOME/.opencode/bin/$agent" "$HOME/.grok/bin/$agent"; do
    if [ -e "$location" ] || [ -L "$location" ]; then
        # Existing but broken/unexecutable installs must not trigger replacement.
        printf 'found\000%s' "$location"
        exit 0
    fi
done
printf 'missing\000%s' "$HOME"
