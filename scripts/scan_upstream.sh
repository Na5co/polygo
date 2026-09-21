#!/usr/bin/env bash
# Run `polygo check` on another project's shipped translations without cloning it all.
#   scripts/scan_upstream.sh <owner/repo> <path-to-catalog-or-res-dir> [branch]
# Prints the error findings (placeholders, plurals) as JSON lines to stdout.
set -euo pipefail
repo="$1"; path="$2"; branch="${3:-}"
work=$(mktemp -d)
proj="$work/proj"; mkdir -p "$proj"
case "$path" in
  *.xcstrings)
    [ -n "$branch" ] || branch=$(gh api "repos/$repo" --jq .default_branch)
    mkdir -p "$proj/App"
    curl -fsSL "https://raw.githubusercontent.com/$repo/$branch/$path" -o "$proj/App/Localizable.xcstrings" ;;
  *)
    git clone -q --depth 1 --filter=blob:none --sparse ${branch:+--branch "$branch"} "https://github.com/$repo" "$work/src"
    (cd "$work/src" && git sparse-checkout set "$path" >/dev/null)
    cp -R "$work/src/$path" "$proj/res" ;;
esac
cd "$proj" && polygo init >/dev/null 2>&1 || { echo "init failed for $repo/$path" >&2; exit 1; }
polygo check --json 2>/dev/null | python3 -c "
import json,sys; r=json.load(sys.stdin)
errs=[f for f in r['findings'] if f['severity']=='error']
print(json.dumps({'repo':'$repo','path':'$path','errors':len(errs),'warnings':r['warnings']}))
for f in errs: print(json.dumps(f, ensure_ascii=False))
"
rm -rf "$work"
