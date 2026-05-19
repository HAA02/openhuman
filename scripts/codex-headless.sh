#!/usr/bin/env bash
# Headless codex wrapper used by openhuman's codex_cli runner.
#
# Resets the environment to a clean baseline (no CARGO_*, no CEF state,
# no inherited FDs) so codex does not deadlock on a futex during startup
# the way it does when spawned inline from the Tauri host process.
#
# Args: $1 = prompt, $2 = output_file
# Optional env: CODEX_CWD (default /tmp), CODEX_MODEL.

set -eu

PROMPT=${1:?prompt required}
OUTPUT=${2:?output file required}
CWD=${CODEX_CWD:-/tmp}

# First pass: re-exec with `env -i` to drop every inherited env var
# except what codex actually needs.
if [ -z "${CODEX_HEADLESS_CLEAN:-}" ]; then
  exec env -i \
    HOME="$HOME" \
    USER="${USER:-$(id -un)}" \
    LOGNAME="${LOGNAME:-${USER:-$(id -un)}}" \
    PATH="/usr/local/bin:/usr/bin:/bin" \
    LANG="${LANG:-en_US.UTF-8}" \
    CODEX_HEADLESS_CLEAN=1 \
    CODEX_CWD="$CWD" \
    ${CODEX_MODEL:+CODEX_MODEL="$CODEX_MODEL"} \
    bash "$0" "$PROMPT" "$OUTPUT"
fi

# Drop ALL non-stdio FDs that may have been inherited despite execve()
# (close-on-exec covers most but not all).
for fd in /proc/self/fd/[0-9]*; do
  n=${fd##*/}
  if [ "$n" -gt 2 ] 2>/dev/null; then
    eval "exec $n>&-" 2>/dev/null || true
  fi
done

args=(
  exec
  --sandbox read-only
  --skip-git-repo-check
  --ignore-user-config
  --ignore-rules
  --cd "$CWD"
  --output-last-message "$OUTPUT"
)
if [ -n "${CODEX_MODEL:-}" ]; then
  args+=(--model "$CODEX_MODEL")
fi
args+=("$PROMPT")

# setsid → fresh session/process group → no inherited controlling tty.
# stdin from /dev/null, stderr passthrough so spawn errors are visible.
exec setsid -w codex "${args[@]}" </dev/null
