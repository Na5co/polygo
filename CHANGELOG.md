# Changelog

All notable changes to polygo. Versions follow [SemVer](https://semver.org); dates are ISO.

## Unreleased

- `check` text output shows the first 20 findings of each code and then `… 3368 more state`, so a catalog with thousands of `needs_review` strings stays readable; `--all` lists every one, `--json` always has them all.
- New `check` warnings: `orphan` (a key the locale file has and the source does not, in every per-locale format; i18next plural forms the locale needs are not orphans), `fuzzy` (gettext `#, fuzzy`: shipped as untranslated at runtime), `state` (`.xcstrings` units marked `needs_review`/`stale`/`new` in Xcode, and keys whose `extractionState` is stale). Penpot's German `.po` has exactly its 4 fuzzy entries flagged; boringnotch ships 3,374 strings Xcode says need review.
- `check` ends with a coverage line when a locale is not fully translated (`coverage: pl 83% (1 of 6 missing)`; worst six locales, then a count), and `--json` gains `coverage: {locale: [translated, total]}`. The job summary from `--github` shows it too.

## 0.1.3 — 2026-09-21

- `check` findings carry the file a fix goes in and the line of the key: `locales/de.json:12` in the text output, `line=` in `--github` annotations (they now land on the exact line of the PR), `"line"` in `--json`. For `.xcstrings` the line is the locale's entry inside the key. Per-locale formats now name the locale file, not the source file.
- New checks: `markup` (error: a `<b>`, `</a>` or `<br>` the source has and the translation lacks, or the reverse), `whitespace` (warning: leading/trailing space or newline dropped or added), `punctuation` (warning: the source ends with `:` `.` `!` `?` `…` and the translation ends with a letter; any script's marks pass), and for Android `escape` (error: unescaped `'`, or a leading `@`/`?` that is not a resource reference, both of which fail `aapt`; warning: a stray unbalanced `"`, which Android silently drops from the text). On the DuckDuckGo macOS catalog these add 26 real warnings; on 3,756 strings from five Android apps, 0 false positives and 3 shipped stray quotes.
- `check` on a large catalog no longer scans the file per finding (IceCubes: 189 s → 3.7 s in a debug build).
- Translation memory no longer learns a hand-edited translation whose placeholders do not match the source, and never answers for keys that `check --fix` / `audit --fix` are re-translating (it could hold exactly the broken text). Found by isolating the test suite from the developer's own memory file.
- `polygo.toml` typos are pointed out: `batch_szie = 5` prints `unknown key \`batch_szie\` (did you mean \`batch_size\`?)` instead of silently doing nothing, and a misspelled required key (`target_locale`) is named in the parse error. Warnings, not errors, so an older polygo still reads a newer file.
- First `polygo translate` with an Ollama model that is not pulled offers to pull it right there (`[Y/n]`); `--yes` for scripts. Without a terminal it says the two ways to pull.
- GitHub Action `mode: check`: validates the repository's translations on every pull request with no model and no secrets, one annotation per finding on the file, a table in the job summary, job fails on errors (`strict: "true"` for warnings too), `path:` for repos without `polygo.toml`. Outputs `errors` / `warnings`.
- `polygo check --github` prints GitHub workflow-command annotations and writes the job summary when `GITHUB_STEP_SUMMARY` is set, for any CI on GitHub.
- The `v0` tag the Action docs pointed at (`Na5co/polygo/action@v0`) did not exist; it does now and moves with every 0.x release.
- Pre-commit snippet in `action/README.md`.
- `polygo check <file-or-directory>` works without `polygo.toml`: the format and locales are detected (a single `res/values/strings.xml` or `locales/en.json` is placed by looking at its parent directories), every validator runs, and the note printed says what was checked. Plain `polygo check` in a project with no config does the same for the current directory. `--fix` still needs a configured project. The ten-second "does my existing localization have bugs?" path, no model involved.
- `init` (and `check`) inside the layout directory itself (`cd res && polygo init`) no longer writes absolute `locale_path` templates.

## 0.1.2 — 2026-09-20

- Release pipeline is back: `scripts/release.sh <version>` bumps, checks, commits and tags; pushing the tag builds five binaries, `SHA256SUMS`, the Homebrew formula (`scripts/formula.sh`, which now installs shell completions) and the GitHub release, and publishes to crates.io and the tap when the secrets exist. CI runs fmt/clippy/tests on every PR. See `docs/dev/RELEASING.md`.
- Failures that retrying cannot fix stop the run at once with the problem and the fix on two lines, the way `doctor` reports: Ollama not running (`ollama serve`), model not pulled (`polygo use <model>`), no or rejected API key (which env var, or `polygo use … --api-key`), unknown model at an API endpoint. Timeouts, 429 and 5xx are still retried three times. Previously every one of these went through three backoff rounds and ended in a chain of socket errors.
- `check` says what it looked at: `check: ok (42 translation(s) in 3 locale(s))`, or `nothing to check yet … polygo translate first` on a fresh project.
- `polygo status --keys` (`-k`) lists the keys behind each count, with the quarantine reason for `needs-review` ones; `--json` gains a `keys` map per locale. `status --locale de` narrows it.
- `[keys] skip = ["debug.*", "internal_*"]` in `polygo.toml`: globs over the key that translate, status and check leave alone, for i18next JSON (no comment field for `polygo:skip`) and for whole families of keys in any format. `status` reports how many keys were skipped.
- Android `<string-array>` items are translated, one unit per item (`sort_modes#array.0`), with the whole list in the prompt so the items stay parallel. A missing array in a locale file is created from the source and filled item by item; a short one is padded before the new item lands, so an array is never left shorter than the original. `translatable="false"` arrays are skipped as before.
- `polygo completions <shell>` prints a completion script for bash, zsh, fish, elvish or PowerShell.
- i18next JSON plurals are translated per CLDR category: `photos_one` / `photos_other` in `en.json` produces `photos_one`, `photos_few`, `photos_many`, `photos_other` for Polish and only `photos_other` for Japanese, written in CLDR order next to the group. Previously only the source's own suffixes were written and `polygo check` then failed on polygo's own output for Slavic, Arabic and other multi-form locales.
- `check` no longer treats `step_one` + `step_two` (no `_other`) or a lone `items_other` as a plural group.
- `polygo add`, `remove` and `use` edit `polygo.toml` in place: comments and formatting survive, `target_locales` stays on one line.
- `init` in a project with no target locales, and `translate` with an empty `target_locales`, now say to run `polygo add <locale>` instead of reporting everything up to date.

## 0.1.1

- `polygo extract`: pull UI text out of JSX, HTML in template literals and .html/.vue/.svelte into `locales/en.json`. `--rewrite` replaces the strings in .ts/.tsx/.js/.jsx with `t("key")` calls and generates `src/i18n.ts`. Sentences with inline markup are extracted whole. `[extract] ignore` / `ignore_paths`, `--ignore`, `--ignore-path`.
- `polygo add` / `polygo remove` edit target locales.
- Batches are capped by source text length and Ollama gets a 16k context, so paragraphs no longer overflow the model.
- `translate` prints the plan and per-batch progress.
- Placeholder mismatches are caught in the repair loop, not only by `check`.
- Fragment check no longer flags HTML entities, handles, paths or hyphenated tokens.
- `init` explains how to start when an app has no string files yet.

## 0.1.0

First release.

- Formats: Xcode `.xcstrings`, Android `strings.xml`, i18next JSON, Flutter ARB, gettext `.po`, .NET `.resx`/`.resw`. Every writer is byte-stable (35 real-world files in `tests/corpus` round-trip exactly).
- `polygo init` detects the project layout and locales for all six formats.
- `polygo translate`: lockfile-driven incremental translation, batches, parallel jobs, crash-safe resume, `--dry-run`, `--retry-review`, `--no-context`. Human-edited translations are never overwritten.
- Context retrieval: code-usage snippets (Aho-Corasick over the source tree) and similar already-translated strings, within a token budget.
- Validation and repair of every model answer: placeholders, glossary, key echo, runaway length, identical-to-source; one repair round, then quarantine as `needs-review`.
- Plural translation: `.xcstrings` `variations.plural` (top-level and `%#@var@` substitutions), Android `<plurals>`, gettext `msgid_plural`. One unit per CLDR category the target locale needs, written into the native structure.
- `polygo check`: printf / ICU / i18next / composite-format placeholders, CLDR plural categories, empty/identical/length, untranslated fragments in non-Latin-script targets; `--json`, `--strict`, `--fix`.
- `polygo doctor`: checks config, files, provider reachability and that the model is pulled, and prints the fix for each failure.
- `polygo audit`: a second model grades translations 1 to 5 with reasons; `--fix` re-translates flagged strings, never human edits.
- Translation memory across projects (`~/.config/polygo/memory.toml`): human entries reused verbatim, model entries as few-shot examples; `polygo memory`.
- `polygo pseudo`: pseudo-locale (`en-XA`) with placeholders preserved.
- `polygo status --markdown`: coverage table with flags; the GitHub Action puts it in the PR body.
- Per-key directives in developer comments: `polygo:skip`, `polygo:max=N`.
- `polygo models` / `polygo use <model>`: model catalog; pulls Ollama models with progress; `openai/...`, `anthropic/...` or `--base-url` for API providers; `--api-key` stored with mode 0600; `--global` default for `init`.
- `polygo status`, `polygo review` (localhost-only approval page), GitHub Action (`Na5co/polygo/action`).
- Providers: Ollama (default, `qwen3:8b`), any OpenAI-compatible endpoint, Anthropic, deterministic mock.
- Distribution: prebuilt binaries for macOS (arm64, x86_64), Linux (musl, arm64, x86_64), Windows; `install.sh`, Homebrew tap, `cargo binstall`.

### Known limitations

- `.xcstrings` device variations are preserved but not translated. (Android `<string-array>` items: see Unreleased above.)
- ICU plural messages inside JSON/ARB strings are translated as a whole; `check` validates their structure but not whether the translation added the categories the locale needs.
- An 8B local model gets some Slavic inflections wrong (live test: 8/8 Polish, 6/8 Russian plural forms); use a larger model for those.
