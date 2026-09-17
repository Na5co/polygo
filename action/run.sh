#!/usr/bin/env bash
# Runs inside the polygo GitHub Action (and locally for tests).
#   POLYGO_BIN      path to the polygo binary (default: polygo on PATH)
#   POLYGO_LOCALES  optional comma-separated locales
#   POLYGO_ARGS     extra args for `polygo translate`
#   DRY_RUN=1       translate + check, print the diff, never commit
set -euo pipefail
BIN="${POLYGO_BIN:-polygo}"
ARGS=()
[ -n "${POLYGO_LOCALES:-}" ] && ARGS+=(--locale "$POLYGO_LOCALES")
# shellcheck disable=SC2206
[ -n "${POLYGO_ARGS:-}" ] && ARGS+=(${POLYGO_ARGS})

echo "::group::polygo status (before)"; "$BIN" status || true; echo "::endgroup::"
set +e
"$BIN" translate ${ARGS[@]+"${ARGS[@]}"}
rc=$?
set -e
# 3 = some strings quarantined for review; still worth committing what was written.
if [ $rc -ne 0 ] && [ $rc -ne 3 ]; then
  echo "::error::polygo translate failed (exit $rc)"; exit $rc
fi
echo "::group::polygo check"; "$BIN" check || echo "::warning::polygo check reported problems"; echo "::endgroup::"

if git diff --quiet && [ -z "$(git ls-files --others --exclude-standard)" ]; then
  echo "no translation changes"
  echo "changed=false" >> "${GITHUB_OUTPUT:-/dev/null}"
  exit 0
fi
echo "changed=true" >> "${GITHUB_OUTPUT:-/dev/null}"
# Coverage table for the PR body (multi-line output).
{ echo "coverage<<POLYGO_EOF"; "$BIN" status --markdown; echo "POLYGO_EOF"; } >> "${GITHUB_OUTPUT:-/dev/null}"
echo "::group::diff"; git --no-pager diff --stat; git --no-pager diff; echo "::endgroup::"
if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "dry run: not committing"
  exit 0
fi
git add -A
git -c user.name="polygo" -c user.email="polygo@users.noreply.github.com" commit -m "chore(i18n): update translations with polygo" -q
echo "committed"
