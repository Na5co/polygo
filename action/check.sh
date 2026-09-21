#!/usr/bin/env bash
# `mode: check` of the polygo GitHub Action (also runnable locally for tests).
#   POLYGO_BIN      path to the polygo binary (default: polygo on PATH)
#   POLYGO_PATH     file or directory to check; empty = the configured project
#   POLYGO_LOCALES  optional comma-separated locales
#   POLYGO_STRICT=1 warnings fail the job too
#   POLYGO_SARIF    path to also write SARIF to (uploaded by the action when set)
# `polygo check --github` prints one annotation per finding and writes the job summary;
# this wrapper only assembles the arguments and exposes the totals as outputs.
set -euo pipefail
BIN="${POLYGO_BIN:-polygo}"
ARGS=()
[ -n "${POLYGO_PATH:-}" ] && ARGS+=("$POLYGO_PATH")
[ -n "${POLYGO_LOCALES:-}" ] && ARGS+=(--locale "$POLYGO_LOCALES")
[ "${POLYGO_STRICT:-0}" = "1" ] && ARGS+=(--strict)

set +e
out=$("$BIN" check --github ${ARGS[@]+"${ARGS[@]}"})
rc=$?
set -e
# The same findings as SARIF for code scanning, when asked (baseline applies to both).
if [ -n "${POLYGO_SARIF:-}" ]; then
  "$BIN" check --sarif ${ARGS[@]+"${ARGS[@]}"} > "$POLYGO_SARIF" || true
fi
printf '%s\n' "$out"
totals=$(printf '%s\n' "$out" | sed -n 's/^polygo check: \([0-9]*\) error(s), \([0-9]*\) warning(s), \([0-9]*\) translation(s).*/\1 \2 \3/p')
[ -n "$totals" ] || { echo "::error::polygo check did not run (exit $rc)"; exit "${rc:-1}"; }
read -r errors warnings checked <<< "$totals"
{ echo "errors=$errors"; echo "warnings=$warnings"; echo "checked=$checked"; } >> "${GITHUB_OUTPUT:-/dev/null}"
exit "$rc"
