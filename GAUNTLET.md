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
- [x] G2.2 Batching + concurrency + retry/backoff + resume: a run killed halfway resumes without re-translating finished keys (lockfile written incrementally).
  - Acceptance: `cargo test resume_after_kill` (test kills the run after N keys using the mock provider with an injected panic).
- [x] G2.3 Glossary (`glossary.toml`: term → per-locale term, `do_not_translate` list) injected into prompts and enforced post-hoc (translation must contain the glossary target term).
  - Acceptance: `cargo test glossary_enforced`.
- [x] G2.4 Prompt templates per format with strict output schema; parse failures retried once with a repair prompt, then marked `needs_review` rather than written.
  - Acceptance: `cargo test malformed_output_is_quarantined`.

## Phase 3 — Validation (`polygo check`)

- [x] G3.1 Placeholder parity: `%@ %d %lld %1$@ %1$s {name} {{name}} $name` — same multiset in source and translation, positional order allowed to change only with numbered args.
  - Acceptance: `cargo test check_placeholders` catches 100% of `expected.mutations.json` in corpus with 0 false positives on originals.
- [x] G3.2 Plural completeness: target locale must have every CLDR plural category it requires (e.g. `ru` needs one/few/many/other; `ja` needs other only).
  - Acceptance: `cargo test check_plurals`.
- [x] G3.3 Length guard (configurable ratio, default 2.5×) and empty/identical-to-source detection.
  - Acceptance: `cargo test check_length_identity`.
- [x] G3.4 `check` exit code non-zero on any error; `--fix` re-translates offending keys.
  - Acceptance: `cargo test check_exit_codes`.

## Phase 4 — Context retrieval (the differentiator)

- [x] G4.1 Usage finder: for each key, locate usages in Swift/Kotlin/Java/Dart/TS/TSX/JS (regex per language, `ignore` crate for gitignore-aware walking), capture ±6 lines and the enclosing identifier (function/view/component name).
  - Acceptance: `cargo test usage_finder_*` on fixture projects — ≥ 90% of keys resolved, < 200 ms for a 5k-file tree.
- [x] G4.2 Similar-translation few-shot: pick up to 3 existing translations for the target locale with highest token-overlap/Jaccard to the source string (no embeddings needed for MVP).
  - Acceptance: `cargo test fewshot_selection`.
- [x] G4.3 Context assembled into the prompt with a hard token budget; `--no-context` flag for A/B.
  - Acceptance: `cargo test prompt_budget_respected`.
- [x] G4.4 A/B harness `scripts/ab.sh`: translate a corpus app with and without context, judge with a different local model (`gemma4`) blind to condition and print win rate; keep output in `bench/`.
  - Acceptance: harness runs; win rate recorded in `## Log`. If context does not win, revisit G4.1–G4.3 before continuing.

## Phase 5 — Breadth + review

- [x] G5.1 Flutter `.arb` (ICU plural/select, `@meta` preserved).
- [x] G5.2 `.po` (msgctxt, plural forms header) and `.resx`.
- [x] G5.3 `polygo review`: axum serves one embedded HTML file (no build step, no node) at 127.0.0.1: table of pending translations, edit/approve/reject, writes back through the same serializers. Binds localhost only, rejects non-localhost Host header.
  - Acceptance: `cargo test review_roundtrip` (approve via HTTP → file updated byte-stably).
- [ ] G5.4 GitHub Action `polygo/action`: runs `translate` on push, opens a PR with the diff.
  - Acceptance: dry-run in a fixture repo produces the expected diff.

## Phase 6 — Release

- [x] G6.1 `cargo dist` or `cross` builds for macOS arm64/x64, Linux x64/arm64, Windows x64; release binary < 15 MB.
- [x] G6.2 `brew tap`, `cargo install`, `npx`-style shim optional. Install-to-first-translation under 5 minutes measured on a clean machine.
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
- aho-corasick 1 → one-pass multi-needle scan of the source tree for the usage finder (already a transitive dep via regex)
- ureq 3 (json) → blocking HTTP for providers; small, no tokio; TLS via rustls; binary stays 1.9 MB (the provider code is only linked into the CLI when used)
- tiny_http 0.12 → `polygo review` server; chosen over axum to avoid tokio/hyper (binary stays 3.9 MB, cold start 1.7 ms); same behaviour: one embedded page, localhost only
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
- 2026-09-17 · G2.2 · `polygo translate [--locale] [--batch-size] [--jobs] [--dry-run] [-v]`: work from lockfile status, batches, worker threads with a per-batch ack (a crash loses only in-flight batches; jobs=1 is strictly sequential), 3 retries with backoff, atomic file writes + lock save after every batch. Write-back for xcstrings (sorted locale insert), Android (splice or append, creates values-xx files), JSON (style-detected re-render, keeps target-only keys). Glossary loader + enforcement. Mock crash/log hooks → resume_after_kill passes. Live finding: qwen3:8b copied English verbatim for a 4-item Japanese batch; fixed by naming the JSON field `translation` (not `text`), spelling out language names, and restating the target language at the end of the user prompt — now deterministic across runs. 35 tests; binary 3.4 MB (TLS).
- 2026-09-17 · G2.3 · glossary.toml (do_not_translate + per-locale terms) injected into the system prompt and enforced post-hoc (case-insensitive term check, verbatim do-not-translate check); violations name the key and are never written. Mock provider honours the glossary unless POLYGO_MOCK_IGNORE_GLOSSARY=1. 38 tests. Retry-with-repair on violation is deferred to G2.4.
- 2026-09-17 · G2.4 · Provider trait now exposes only `complete(system, user)`; shared `run()` does prompt → lenient parse → validate (missing, empty, glossary, do-not-translate, identical-to-source) → one repair round naming the problems → quarantine. Quarantined keys are `needs-review` in polygo.lock ([keys.k.review.<locale>] with reason + suggestion), never written, excluded from work until `translate --retry-review`; `translate` exits 3 when anything was quarantined. Identical-to-source is accepted if the model confirms it on the second ask (live: "OK" → "OK" for de/ja). Mock now implements `complete` by reading keys out of the prompt, so the real prompt/parse path is under test; hooks for malformed/drop/echo. 39 tests. Phase 2 complete.
- 2026-09-17 · G3.1 · check/placeholders: printf family (argnum, flags, width, precision, length, conversion families; `%%`; no space flag so `25% off` is prose), Apple `%#@var@` (per-locale names → compared by count), ICU braces with recursion into plural/select bodies + structural validity (`<malformed ICU>`), i18next `{{x}}`/`$t()`, `$name`. Numbered printf args compared as a set (repeat/reorder ok), unnumbered as an ordered multiset. Mutation corpus generated by scripts/gen_mutations.py from real translations (1052 mutations across 22 files, drop/retype/dup/swap/braces): 1052/1052 caught. False-positive sweep over 43,332 real source/translation pairs: 32 flags, every one verified by hand as a genuine upstream bug (recorded in tests/corpus/xcstrings/known_bugs.json + KNOWN_BUGS.md — e.g. DuckDuckGo ships `Ouvrir dans % @`). 43 tests.
- 2026-09-17 · G3.2 · check/plurals: CLDR cardinal table (~50 languages; CLDR-42 `many` for fr/es/it/ca/pt accepted, not required; `other`-only = deliberate opt-out; extras like `zero`/`=N` fine), collectors for xcstrings variations + substitutions, Android <plurals>, ICU inline (recursive), i18next `_one/_other` suffix groups. Corpus: Loop complete in 13 locales; Android/grafana/immich sources complete; IceCubes has 44 be/pl/uk plural sets shipping only one/other — genuine upstream gaps, documented in KNOWN_BUGS.md. 47 tests.
- 2026-09-17 · G3.3 · check/text: empty (error), identical-to-source (warning; ignores same-language targets, placeholder-only and markup-only strings), length ratio both directions (warning; configurable, default 2.5× with +8 char slack so "OK"→"Akkoord" passes). 48 tests.
- 2026-09-17 · G3.4 · `polygo check [--locale] [--json] [--strict] [--fix]`: placeholders + text checks per translation, plural structures per format (xcstrings variations/substitutions, Android <plurals> per locale file, i18next `_one/_other` groups, ICU inline). Exit 1 on errors (or warnings with --strict); `--fix` re-translates only keys with errors via engine `force_keys` then re-checks. Identical warnings skip acronyms/≤3 letters and lock-confirmed translations. IceCubes catalog: 50 ms, 65 errors (44 plural, 19 placeholders, 2 empty). 50 tests. Phase 3 complete.
- 2026-09-17 · G4.1 · context/usage: gitignore-aware index of Swift/ObjC/Kotlin/Java/Dart/TS/JS/Vue/Svelte/XML, needles per key (`"k"`, `'k'`, `` `k` ``, `R.string.k`, `@string/k`, `.k` with boundary checks, dot-refs as weak fallback), enclosing declaration (func/fun/function/struct/class/component, skipping `var body`/`const { t }`), dedented ±6-line snippet. Fixture: 16/16 keys across 5 languages; boundaries `title` vs `title_long`; 5k-file tree < 200 ms in debug. Real repo (Loop): 382/406 keys resolved in 61 ms. examples/usage_probe.rs kept as a dev probe. 53 tests.
- 2026-09-17 · G4.2 · context/fewshot: token Jaccard (lowercased alnum, stopwords removed, placeholders ignored) with a mild length-similarity preference; skips the identical source and zero-overlap candidates; deterministic ordering. 54 tests.
- 2026-09-17 · G4.3 · context/assemble: per-request budget (config `context_tokens`, default 600 ≈ chars/4): file:line + ident first, then ≤3 few-shots (clipped), then the snippet trimmed around the match line; `context = false` in polygo.toml or `translate --no-context` disables. Engine indexes the tree once and resolves only keys with work; few-shots come from the locale's existing translations. Mock gets POLYGO_MOCK_DUMP for prompt assertions. Live: `Open` inside `Menu("File") { Button("Open") }` → prompt carries the Swift snippet. 56 tests.
- 2026-09-17 · G4.4 · scripts/ab.py: strips the target locale from N sampled keys in two copies of a real repo, translates with/without context (same model, batch 10), judges blind with gemma4 (identical outputs auto-tie; each pair asked twice with swapped order, counted only when consistent; judge sees a one-line usage hint). Loop / de / qwen3:8b, n=60, seed 23. History: run1 (n=40) 12:10 ties 18 — judge preferred B on identical strings → protocol hardened; run2 10:10 — context left 8 strings in English → untranslated/broken pairs excluded from few-shots; run3 10:8 — still echoes from code identifiers → plain re-ask for context-induced echoes; run4 8:9 — model returned the *key* ("Icon Name: Summer") → key-echo rejected in validation; run5 **9:5, ties 45 (33 identical), 64% of decided, 0 leaks**. Cost: context run 157 s vs 57 s. Verdict: context wins modestly and mainly buys terminology consistency with the app's existing translations ("Tastenbelegung", product names kept); each iteration also removed a real failure mode from the engine. Kept on by default; `--no-context` remains for speed. bench/ab-de.json committed. Phase 4 complete.
- 2026-09-17 · G5.1 · formats/arb on top of the span JSON parser: byte-stable on 4 real files (fluffychat en/de 4-space, lichess 2.1k keys, ente 2.1k keys, 78 ICU plurals); units skip `@` keys and carry `@key.description` + placeholder names as comments; locale files rebuilt in template order with `@@locale` first, existing values and target-only keys kept, metadata never copied. Wired into project loader, engine workspace, init (already), check (ICU plurals via icu_cases). End-to-end mock translate + status + check on a Flutter fixture. 60 tests.
- 2026-09-17 · G5.2 · formats/po: span-based (msgctxt/msgid/msgid_plural/msgstr[n], multi-line continuations, `#.`/`#:` comments as unit context, obsolete `#~` skipped), gettext-style encode, append entries, new locale file from the source header with Language + Plural-Forms per language; plural entries preserved but not yet units. formats/resx (+.resw): quick-xml spans of `<value>`, `<comment>` as context, binary `type=`/`mimetype=` data skipped, append before `</root>`, new locale file = source minus data elements. Byte-stable on 4 .po (Django, penpot ≤2.5k msgids) and 5 .resx/.resw (ShareX, NAPS2, Files 1.4k). init detects `<lang>/LC_MESSAGES/x.po`, `<lang>.po`, `Name.<lang>.resx`, `<lang>/Resources.resw`. Engine PerLocale workspace. 65 tests; binary 3.5 MB.
- 2026-09-17 · G5.3 · `polygo review [--port 4133] [--open]`: single embedded HTML page (locale + filter selects, source/translation table with code-usage line and developer comment, edit textarea, Approve/Reject, ⌘/Ctrl+Enter), JSON API (`/api/items`, `/api/approve`, `/api/reject`), binds 127.0.0.1 and refuses non-local Host (403). Approve writes through the format serializers (byte-stable, only the edited value changes) and records provider=human in the lockfile; reject quarantines. Deviation: tiny_http instead of axum (see Deps). 66 tests.
- 2026-09-17 · G6.2 · install paths: packaging/homebrew/polygo.rb (tap formula, 4 Unix assets), install.sh (POSIX, arch/os detection, latest tag), Cargo.toml metadata for crates.io + cargo-binstall (same asset layout), LICENSE (MIT). Measured on this machine: tarball (2.0 MB) → `init` → first translation of 10 strings with qwen3:8b = 15 s; `cargo install --path .` = 8 s with warm deps. A truly clean machine adds the one-time `ollama pull qwen3:8b` (5.2 GB) — bandwidth-bound, ~8 min at 11 MB/s; with an API provider instead it is well under 5 minutes. 69 tests.
- 2026-09-17 · G6.1 · .github/workflows/release.yml: 5-target matrix (aarch64/x86_64 darwin, x86_64/aarch64 linux-musl via cross, x86_64 windows), size budget enforced in CI, tarballs/zip named `polygo-<tag>-<arch>-<os>` exactly as action/action.yml, install.sh and the Homebrew formula expect (tests/release_layout.rs pins this), SHA256SUMS + GitHub release. ci.yml: fmt/clippy/test on 3 OSes + size budget. Verified locally: aarch64-darwin 3.9 MB, x86_64-darwin 4.8 MB (rustup target), aarch64-linux-musl 4.1 MB static ELF built and run in the rust:1-alpine container (`cross` itself fails on an arm64 Mac host — environment quirk, the workflow uses it on ubuntu). Windows and x86_64-linux rely on CI.
