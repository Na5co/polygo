#!/usr/bin/env bash
# Cut a release locally, then let GitHub Actions build it:
#   scripts/release.sh 0.1.2
# Runs the same checks as CI, bumps Cargo.toml (and Cargo.lock), dates the "Unreleased"
# section of CHANGELOG.md, commits "polygo 0.1.2" and tags v0.1.2. Nothing is pushed:
# review with `git show`, then `git push --follow-tags` and release.yml does the rest.
set -euo pipefail
cd "$(dirname "$0")/.."
v="${1:?usage: scripts/release.sh <version>   (e.g. 0.1.2)}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not a semver version: $v" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "working tree is not clean" >&2; exit 1; }
[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || echo "note: releasing from branch $(git rev-parse --abbrev-ref HEAD), not main" >&2
git tag | grep -qx "v$v" && { echo "tag v$v already exists" >&2; exit 1; }
grep -q '^## Unreleased' CHANGELOG.md || { echo "CHANGELOG.md has no '## Unreleased' section" >&2; exit 1; }

echo "checks…"
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -q 2>&1 | grep -E "^test result: FAILED|panicked" && { echo "tests failed" >&2; exit 1; }

# From here on, a failure puts the files back.
trap 'git checkout -q -- Cargo.toml Cargo.lock CHANGELOG.md; echo "aborted: files restored" >&2' ERR
perl -pi -e '$done ||= s/^version = "[^"]*"/version = "'"$v"'"/' Cargo.toml   # first match only
cargo update -q -w
perl -pi -e 's/^## Unreleased$/## '"$v"' — '"$(date +%Y-%m-%d)"'/' CHANGELOG.md
grep -q "^version = \"$v\"" Cargo.toml
grep -q "^## $v — " CHANGELOG.md

cargo build --release -q
bin="${CARGO_TARGET_DIR:-target}/release/polygo"
size=$(wc -c < "$bin"); [ "$size" -lt 15728640 ] || { echo "binary is $size bytes, over the 15 MB budget" >&2; false; }
"$bin" --version | grep -q "$v"
trap - ERR

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -q -m "polygo $v"
git tag -a "v$v" -m "polygo $v"
echo
echo "tagged v$v. Review:   git show --stat HEAD"
echo "Publish:              git push --follow-tags"
echo "release.yml builds the binaries, SHA256SUMS, the GitHub release and the formula;"
echo "cargo publish and the Homebrew tap run when CARGO_REGISTRY_TOKEN / TAP_GITHUB_TOKEN are set."
