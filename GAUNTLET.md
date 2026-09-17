# polygo — Gauntlet

Single-binary, git-native localization CLI for solo devs. Rust. BYOM (default `ollama/qwen3:8b`).
Pitch: "Lokalise for one person. Runs in your repo, knows your code, never phones home."

This file is the loop's state. One gate at a time. A gate is DONE only when its
acceptance command passes. Never edit acceptance commands to make them pass.

## Rules for the loop

1. Read this file. Find the first gate not marked `[x]`. Work only on that gate.
2. Before writing code, write/extend the test that the acceptance command runs.
3. Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test` plus the gate's acceptance command.
4. If green: mark `[x]`, append one line to `## Log` (date, gate, what changed), commit `gate(N): <name>`.
5. If red after 3 attempts on the same failure: write the failure under `## Blocked`, commit WIP on a branch `blocked/gate-N`, move to the next gate that does not depend on it.
6. Never add a dependency without noting why in `## Deps`. Keep the release binary < 15 MB and cold start < 50 ms (`hyperfine 'polygo --help'`).
7. Never call a paid API in tests. Tests use the `mock` provider (deterministic) or a recorded fixture.
8. No network in `cargo test`. Provider integration is behind `--features live`.
9. Stop and report when all gates in the current phase are `[x]` or blocked.

## Corpus (build once, keep in `tests/corpus/`)

- 10 real `.xcstrings` from public iOS repos (mix of plurals, `%@`, `%lld`, `%1$@`, device variations, stale keys).
- 5 real Android `strings.xml` with `<plurals>`, `%1$s`, `\'` escapes, `translatable="false"`.
- 3 Flutter `.arb` with ICU plurals/selects and `@key` metadata.
- 5 i18next JSON (nested + flat), 2 `.po`, 2 `.resx`.
- For each: the source file, a byte-identical roundtrip expectation, and a hand-made `expected.mutations.json` listing placeholder/plural breaks the validator must catch.

Corpus sources are recorded in `tests/corpus/SOURCES.md` with repo URL + license.

## Phase 0 — Kill test (human gate)

- [ ] G0.1 Script `scripts/killtest.sh`: takes one `.xcstrings` + target locale, runs Qwen3-8B via Ollama twice — (a) key + source string only, (b) with code-usage context + 3 similar existing translations — writes `killtest/<locale>.csv` with both outputs side by side, 30 rows, shuffled, blinded.
  - Acceptance: file exists with 30 rows; human rates it. Record the score in `## Log`.
  - Kill rule: if (b) does not beat (a) on ≥ 18/30 rows, STOP the gauntlet and report.

## Phase 1 — Formats + lockfile

- [ ] G1.1 `polygo` crate skeleton: `clap` CLI with `translate`, `check`, `status`, `review`, `init`. `--help` under 50 ms.
  - Acceptance: `hyperfine --warmup 3 'target/release/polygo --help'` mean < 50 ms.
- [ ] G1.2 `.xcstrings` parser + serializer, byte-stable roundtrip (key order, indentation, `extractionState`, plural variations, device variations preserved).
  - Acceptance: `cargo test roundtrip_xcstrings` passes on all 10 corpus files (diff is empty).
- [ ] G1.3 Android `strings.xml` parser + serializer (plurals, string-arrays, escapes, `translatable=false` skipped, comments preserved).
  - Acceptance: `cargo test roundtrip_android`.
- [ ] G1.4 i18next JSON (nested/flat, key sorting preserved).
  - Acceptance: `cargo test roundtrip_json`.
- [ ] G1.5 Lockfile `polygo.lock` (TOML): per key → blake3(source) + per-locale blake3(translation) + provider/model + timestamp. `status` prints new/changed/stale/untranslated counts per locale.
  - Acceptance: `cargo test lockfile_*` — changing one source string marks exactly one key stale in every locale.
- [ ] G1.6 `polygo init` writes `polygo.toml` (source locale, targets, file globs, provider, glossary path) by detecting the project type from the file tree.
  - Acceptance: `cargo test init_detects_*` for an iOS, Android, Flutter, and Next.js fixture tree.

## Phase 2 — Translation engine

- [ ] G2.1 `Provider` trait: `translate(batch: &[Unit], ctx: &Ctx) -> Result<Vec<Translation>>`. Implement `mock` (deterministic, reversible), `ollama` (`/api/chat`, JSON schema output), `openai_compatible` (any base URL), `anthropic`.
  - Acceptance: `cargo test provider_mock_*`; `cargo test --features live provider_ollama_smoke` (manual).
- [ ] G2.2 Batching + concurrency + retry/backoff + resume: a run killed halfway resumes without re-translating finished keys (lockfile written incrementally).
  - Acceptance: `cargo test resume_after_kill` (test kills the run after N keys using the mock provider with an injected panic).
- [ ] G2.3 Glossary (`glossary.toml`: term → per-locale term, `do_not_translate` list) injected into prompts and enforced post-hoc (translation must contain the glossary target term).
  - Acceptance: `cargo test glossary_enforced`.
- [ ] G2.4 Prompt templates per format with strict output schema; parse failures retried once with a repair prompt, then marked `needs_review` rather than written.
  - Acceptance: `cargo test malformed_output_is_quarantined`.

## Phase 3 — Validation (`polygo check`)

- [ ] G3.1 Placeholder parity: `%@ %d %lld %1$@ %1$s {name} {{name}} $name` — same multiset in source and translation, positional order allowed to change only with numbered args.
  - Acceptance: `cargo test check_placeholders` catches 100% of `expected.mutations.json` in corpus with 0 false positives on originals.
- [ ] G3.2 Plural completeness: target locale must have every CLDR plural category it requires (e.g. `ru` needs one/few/many/other; `ja` needs other only).
  - Acceptance: `cargo test check_plurals`.
- [ ] G3.3 Length guard (configurable ratio, default 2.5×) and empty/identical-to-source detection.
  - Acceptance: `cargo test check_length_identity`.
- [ ] G3.4 `check` exit code non-zero on any error; `--fix` re-translates offending keys.
  - Acceptance: `cargo test check_exit_codes`.

## Phase 4 — Context retrieval (the differentiator)

- [ ] G4.1 Usage finder: for each key, locate usages in Swift/Kotlin/Java/Dart/TS/TSX/JS (regex per language, `ignore` crate for gitignore-aware walking), capture ±6 lines and the enclosing identifier (function/view/component name).
  - Acceptance: `cargo test usage_finder_*` on fixture projects — ≥ 90% of keys resolved, < 200 ms for a 5k-file tree.
- [ ] G4.2 Similar-translation few-shot: pick up to 3 existing translations for the target locale with highest token-overlap/Jaccard to the source string (no embeddings needed for MVP).
  - Acceptance: `cargo test fewshot_selection`.
- [ ] G4.3 Context assembled into the prompt with a hard token budget; `--no-context` flag for A/B.
  - Acceptance: `cargo test prompt_budget_respected`.
- [ ] G4.4 A/B harness `scripts/ab.sh`: translate a corpus app with and without context, judge with a strong model (LLM-as-judge) and print win rate; keep output in `bench/`.
  - Acceptance: harness runs; win rate recorded in `## Log`. If context does not win, revisit G4.1–G4.3 before continuing.

## Phase 5 — Breadth + review

- [ ] G5.1 Flutter `.arb` (ICU plural/select, `@meta` preserved).
- [ ] G5.2 `.po` (msgctxt, plural forms header) and `.resx`.
- [ ] G5.3 `polygo review`: axum serves one embedded HTML file (no build step, no node) at 127.0.0.1: table of pending translations, edit/approve/reject, writes back through the same serializers. Binds localhost only, rejects non-localhost Host header.
  - Acceptance: `cargo test review_roundtrip` (approve via HTTP → file updated byte-stably).
- [ ] G5.4 GitHub Action `polygo/action`: runs `translate` on push, opens a PR with the diff.
  - Acceptance: dry-run in a fixture repo produces the expected diff.

## Phase 6 — Release

- [ ] G6.1 `cargo dist` or `cross` builds for macOS arm64/x64, Linux x64/arm64, Windows x64; release binary < 15 MB.
- [ ] G6.2 `brew tap`, `cargo install`, `npx`-style shim optional. Install-to-first-translation under 5 minutes measured on a clean machine.
- [ ] G6.3 README: GIF at line 1 (record with `vhs`), the Lokalise pricing sentence with link, format table, "runs fully offline with Ollama" proof, security note (no telemetry, localhost only).
- [ ] G6.4 Docs page per format; `polygo --help` examples; CHANGELOG.
- [ ] G6.5 Launch kit in `launch/`: r/iOSProgramming, r/FlutterDev, r/androiddev, r/reactjs posts (each leads with that community's format), Show HN title + first comment, Terminal Trove submission, awesome-lists PRs.

## Deps
(record crate → reason)

## Blocked
(gate → failure → what was tried)

## Log
(date · gate · note)
