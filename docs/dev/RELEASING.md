# Releasing

Everything is driven by a tag. Actions is free for this public repository.

```sh
git checkout main && git pull
scripts/release.sh 0.1.2     # checks, bumps Cargo.toml/Cargo.lock, dates CHANGELOG "Unreleased", commits, tags v0.1.2
git show --stat HEAD         # look
git push --follow-tags       # release.yml takes over
```

`release.yml` builds macOS (arm64, x86_64), Linux musl (arm64, x86_64) and Windows, checks the
15 MB budget, writes `SHA256SUMS`, generates the Homebrew formula with `scripts/formula.sh`, and
creates the GitHub release with that version's CHANGELOG section as the notes. A tag with a `-`
(`v0.2.0-rc.1`) is marked pre-release and skips the two publish jobs.

Two optional secrets (Settings → Secrets and variables → Actions):

| secret | job | without it |
|---|---|---|
| `CARGO_REGISTRY_TOKEN` | `cargo publish --locked` | run `cargo publish` by hand |
| `TAP_GITHUB_TOKEN` (write access to `Na5co/homebrew-tap`) | commits `Formula/polygo.rb` to the tap | copy `polygo.rb` from the release assets into the tap by hand |

`ci.yml` runs fmt, clippy, tests and the size budget on one Linux runner for every PR and push to
`main`; superseded runs are cancelled.

Before 0.1.2 the workflows had been removed and 0.1.1 was built by hand; `install.sh`,
`cargo binstall`, the GitHub Action and the formula all expect the asset names
`polygo-v<version>-<arch>-<os>.tar.gz` (`.zip` on Windows), pinned by `tests/release_layout.rs`.
