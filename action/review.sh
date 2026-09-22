#!/usr/bin/env bash
# `mode: check` with `review: true`: post the findings as a pull-request review, with a
# committable suggestion for every mechanical fix. Needs `pull-requests: write`.
#   POLYGO_BIN      path to the polygo binary (default: polygo on PATH)
#   POLYGO_PATH     file or directory to check; empty = the configured project
#   POLYGO_LOCALES  optional comma-separated locales
#   POLYGO_STRICT=1 warnings are errors in the review too
#   GH_TOKEN        token the review is posted with
# The diff comes from the API rather than from git: the lines GitHub accepts a comment on
# are exactly the ones it shows, and a shallow checkout has no base commit to diff against.
set -euo pipefail
BIN="${POLYGO_BIN:-polygo}"
[ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "$GITHUB_EVENT_PATH" ] || { echo "::notice::review: not a pull_request event, skipped"; exit 0; }
PR=$(jq -r '.pull_request.number // empty' "$GITHUB_EVENT_PATH")
[ -n "$PR" ] || { echo "::notice::review: not a pull_request event, skipped"; exit 0; }
REPO="${GITHUB_REPOSITORY:?}"

# The pull request's diff, as one unified diff polygo can read.
diff=$(gh api "repos/$REPO/pulls/$PR/files" --paginate \
  --jq '.[] | select(.patch != null) | "--- a/\(.filename)\n+++ b/\(.filename)\n\(.patch)"')

ARGS=()
[ -n "${POLYGO_PATH:-}" ] && ARGS+=("$POLYGO_PATH")
[ -n "${POLYGO_LOCALES:-}" ] && ARGS+=(--locale "$POLYGO_LOCALES")
[ "${POLYGO_STRICT:-0}" = "1" ] && ARGS+=(--strict)

set +e
payload=$(printf '%s\n' "$diff" | "$BIN" check --review --diff - ${ARGS[@]+"${ARGS[@]}"})
set -e   # check exits 1 when it finds errors; the review is still what it printed
jq -e . >/dev/null 2>&1 <<<"$payload" || { echo "::error::polygo check --review produced no review"; exit 1; }

# Say a thing once: comments polygo already left on the same file and line are dropped, so
# a second push does not repeat itself.
seen=$(gh api "repos/$REPO/pulls/$PR/comments" --paginate \
  --jq '[.[] | select(.body | contains("<!-- polygo -->")) | "\(.path):\(.line // .original_line)"]' \
  | jq -s 'add // []')
payload=$(jq --argjson seen "$seen" \
  '.comments |= map(select(("\(.path):\(.line)") as $k | ($seen | index($k)) | not))' <<<"$payload")

n=$(jq '.comments | length' <<<"$payload")
if [ "$n" -eq 0 ]; then
  echo "::notice::polygo review: nothing new to say on this diff"
  exit 0
fi
gh api --method POST "repos/$REPO/pulls/$PR/reviews" --input - <<<"$payload" >/dev/null
echo "::notice::polygo review: $n comment(s) posted"
