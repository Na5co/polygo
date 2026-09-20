# Changelog

All notable changes to polygo. Versions follow [SemVer](https://semver.org); dates are ISO.

## Unreleased

- Failures that retrying cannot fix stop the run at once with the problem and the fix on two lines, the way `doctor` reports: Ollama not running (`ollama serve`), model not pulled (`polygo use <model>`), no or rejected API key (which env var, or `polygo use … --api-key`), unknown model at an API endpoint. Timeouts, 429 and 5xx are still retried three times. Previously every one of these went through three backoff rounds and ended in a chain of socket errors.
- `check` says what it looked at: `check: ok (42 translation(s) in 3 locale(s))`, or `nothing to check yet … polygo translate first` on a fresh project.
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
