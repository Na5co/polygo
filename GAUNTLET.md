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
9. There are NO human gates. Never wait for a person. The loop stops only when every gate is `[x]` or blocked.
10. Self-review before marking `[x]`: re-read the diff as a hostile reviewer (correctness, edge cases, byte-stability, error messages, docs). Fix what you find, then re-run the acceptance command. Only then mark `[x]`.
11. Judging quality (translations, README copy, prompts) is done by a second model, never by the one that produced the output: use `ollama run gemma4` (or any non-Qwen local model) as judge, with a blinded A/B prompt, and record the numbers in `## Log`.

## Corpus (build once, keep in `tests/corpus/`)

- 10 real `.xcstrings` from public iOS repos (mix of plurals, `%@`, `%lld`, `%1$@`, device variations, stale keys).
- 5 real Android `strings.xml` with `<plurals>`, `%1$s`, `\'` escapes, `translatable="false"`.
- 3 Flutter `.arb` with ICU plurals/selects and `@key` metadata.
- 5 i18next JSON (nested + flat), 2 `.po`, 2 `.resx`.
- For each: the source file, a byte-identical roundtrip expectation, and a hand-made `expected.mutations.json` listing placeholder/plural breaks the validator must catch.

Corpus sources are recorded in `tests/corpus/SOURCES.md` with repo URL + license.

## Phase 0 — Kill test (AI-judged)

- [x] G0.1 Script `scripts/killtest.sh`: takes one `.xcstrings` + target locale, runs Qwen3-8B via Ollama twice — (a) key + source string only, (b) with code-usage context + 3 similar existing translations — writes `killtest/<locale>.csv` with both outputs side by side, 30 rows, shuffled, blinded. Then `scripts/killtest.py --judge killtest/<locale>.csv --judge-model gemma4` has a different model pick A/B/tie per row (blind to which is which) and `--score` applies the kill rule.
  - Acceptance: `python3 scripts/killtest.py --judge killtest/<locale>.csv --judge-model gemma4 && python3 scripts/killtest.py --score killtest/<locale>.csv` prints PASS on at least one of two locales: one without existing translations (code context only) and one with (e.g. `de`, so few-shots also apply).
  - Kill rule: if neither locale passes, mark the gate blocked with the numbers, and continue the gauntlet anyway with context retrieval demoted to an experimental flag (`--context`) rather than the headline feature.

## Phase 1 — Formats + lockfile

- [x] G1.1 `polygo` crate skeleton: `clap` CLI with `translate`, `check`, `status`, `review`, `init`. `--help` under 50 ms.
  - Acceptance: `hyperfine --warmup 3 'target/release/polygo --help'` mean < 50 ms.
- [x] G1.2 `.xcstrings` parser + serializer, byte-stable roundtrip (key order, indentation, `extractionState`, plural variations, device variations preserved).
  - Acceptance: `cargo test roundtrip_xcstrings` passes on all 10 corpus files (diff is empty).
- [x] G1.3 Android `strings.xml` parser + serializer (plurals, string-arrays, escapes, `translatable=false` skipped, comments preserved).
  - Acceptance: `cargo test roundtrip_android`.
- [x] G1.4 i18next JSON (nested/flat, key sorting preserved).
  - Acceptance: `cargo test roundtrip_json`.
- [x] G1.5 Lockfile `polygo.lock` (TOML): per key → blake3(source) + per-locale blake3(translation) + provider/model + timestamp. `status` prints new/changed/stale/untranslated counts per locale.
  - Acceptance: `cargo test lockfile_*` — changing one source string marks exactly one key stale in every locale.
- [x] G1.6 `polygo init` writes `polygo.toml` (source locale, targets, file globs, provider, glossary path) by detecting the project type from the file tree.
  - Acceptance: `cargo test init_detects_*` for an iOS, Android, Flutter, and Next.js fixture tree.

## Phase 2 — Translation engine

- [x] G2.1 `Provider` trait: `translate(batch: &[Unit], ctx: &Ctx) -> Result<Vec<Translation>>`. Implement `mock` (deterministic, reversible), `ollama` (`/api/chat`, JSON schema output), `openai_compatible` (any base URL), `anthropic`.
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
- [ ] G4.4 A/B harness `scripts/ab.sh`: translate a corpus app with and without context, judge with a different local model (`gemma4`) blind to condition and print win rate; keep output in `bench/`.
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
  - Acceptance: `scripts/review_readme.sh` — a second model (`gemma4`) answers a fixed rubric (what is it / who is it for / how do I install in one command / what formats / does it phone home) from the README alone; all five answered correctly.
- [ ] G6.4 Docs page per format; `polygo --help` examples; CHANGELOG.
- [ ] G6.5 Launch kit in `launch/`: r/iOSProgramming, r/FlutterDev, r/androiddev, r/reactjs posts (each leads with that community's format), Show HN title + first comment, Terminal Trove submission, awesome-lists PRs. Drafts only — the loop never posts anything.
  - Acceptance: files exist; each post under 300 words; a second model rates each ≥ 4/5 on "would a maintainer of that subreddit remove this as spam?" (5 = clearly not).

## Deps
(record crate → reason)
- clap 4 (derive) → CLI parsing; industry standard, tiny cold-start cost (2 ms measured)
- serde_json (preserve_order) → JSON tree with insertion-order maps; Xcode key order is not code-point sorted, so order must be preserved, not recomputed
- anyhow → error plumbing in a binary crate
- blake3 → content hashes for source/translation change detection (fast, 32-hex truncated)
- serde + toml 0.8 → polygo.toml config and polygo.lock parsing; lock is emitted by hand so key/locale order is deterministic
- tempfile (dev) → CLI integration tests in throwaway project dirs
- ignore 0.4 → gitignore-aware directory walking for init (and later the usage finder); pulls regex, binary grows to 1.9 MB — still under the 15 MB budget
- ureq 3 (json) → blocking HTTP for providers; small, no tokio; TLS via rustls; binary stays 1.9 MB (the provider code is only linked into the CLI when used)
- quick-xml 0.37 → tolerant XML tokenizer with byte positions; we never re-emit XML, we splice edited value spans into the original text

## Blocked
(gate → failure → what was tried)

## Log
(date · gate · note)
- 2026-09-17 · G0.1 · killtest/bg.csv generated (30 rows, MrKai77/Loop Localizable.xcstrings, qwen3:8b, code context on 30/30, no existing bg translations so 0 few-shots). Judged blind by gemma4: context wins 8 / losses 13 / ties 9 → FAIL.
- 2026-09-17 · G0.1 · killtest/de.csv (30 rows, same file, code context + 3 few-shot existing de translations). Judged blind by gemma4: wins 18 / losses 5 / ties 7 → PASS. Gate passed on de. Finding: code context alone does not win; code context + similar existing translations does. Phase 4 must ship both together; the few-shot half is not optional.
- 2026-09-17 · G1.1 · clap skeleton with translate/check/status/review/init; tests/cli.rs (4 tests); hyperfine --help mean 2.0 ms; release binary 551 KB (lto, strip, panic=abort).
- 2026-09-17 · G1.2 · xcstrings parse/serialize, byte-stable on 11/11 corpus files (Loop, Whisky, IceCubes, boring.notch, damus, 6× DuckDuckGo; 2.5 KB–3.8 MB) + synthetic edge cases (escapes, empty objects, bool/int, trailing newline, CRLF). Corpus fixtures + licenses in tests/corpus/SOURCES.md (dropped Cork/Pearcleaner: restrictive licenses). Release binary 551 KB (lib not yet linked into main).
- 2026-09-17 · G1.3 · Android strings.xml: span-based model (original text + value spans), byte-stable on 5/5 corpus files (AntennaPod, NewPipe, F-Droid, K-9/Thunderbird, DuckDuckGo; 44–82 KB, plurals, CDATA+HTML, `\'`, translatable=false, preceding-comment capture); minimal-diff edits incl. empty-element rewrite and CDATA preservation; decode/encode for entities + Android escapes. 10 tests green.
- 2026-09-17 · G1.4 · i18next JSON: own position-tracking parser (no deps), span-splice serializer; byte-stable on 6/6 corpus files (excalidraw, hoppscotch — mixes raw Unicode with \u2026 escapes, jellyfin-web 4-space, immich, grafana 780 KB depth 8, formbricks); nested + flat dotted keys, arrays as index paths, BOM/CRLF preserved, minimal-diff edits; cross-checked every entry against serde_json. 14 tests green.
- 2026-09-17 · G1.5 · core::Unit model, config (polygo.toml), lockfile (sorted TOML, blake3 hashes, provider/model/at), project loader for xcstrings/android/json incl. per-locale files, `polygo status [--json]`. States: new / stale / untranslated / edited / up-to-date; pre-existing human translations are `edited` and never overwritten (caught by smoke test on Loop: 399 de translations protected). 20 tests green; binary 999 KB; --help 1.5 ms.
- 2026-09-17 · G1.6 · `polygo init [--force]` detects iOS String Catalogs (source + locales read from the file), Android values dirs (pt-rBR/b+sr+Latn qualifiers mapped both ways), Flutter ARB (l10n.yaml template-arb-file), i18next namespace dirs and flat locale files; skips node_modules/Pods/build/etc. 7 tests incl. CLI refuse-overwrite; smoke on Loop found 13 locales. Phase 1 complete: 27 tests, binary 1.9 MB, --help 1.5 ms.
- 2026-09-17 · G2.1 · provider trait + Request/Ctx/Translation; backends: mock (deterministic `⟦loc⟧ src`, reversible, fail-injection), ollama (/api/chat, JSON-schema `format`, think:false), openai-compatible (json_schema response_format, works for llama.cpp/vLLM/LM Studio/OpenRouter), anthropic (messages API). Lenient JSON extraction (fenced/prose/shorthand) with missing-key errors. Live smoke on qwen3:8b: 2 strings, placeholders %@/%lld preserved, 6.8 s. 31 tests green.
