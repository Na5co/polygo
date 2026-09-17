# Changelog

All notable changes to polygo. Versions follow [SemVer](https://semver.org); dates are ISO.

## 0.1.0 — unreleased

First release.

- Formats: Xcode `.xcstrings`, Android `strings.xml`, i18next JSON, Flutter ARB, gettext `.po`, .NET `.resx`/`.resw`. Every writer is byte-stable (35 real-world files in `tests/corpus` round-trip exactly).
- `polygo init` detects the project layout and locales for all six formats.
- `polygo translate`: lockfile-driven incremental translation, batches, parallel jobs, crash-safe resume, `--dry-run`, `--retry-review`, `--no-context`. Human-edited translations are never overwritten.
- Context retrieval: code-usage snippets (Aho-Corasick over the source tree) and similar already-translated strings, within a token budget.
- Validation and repair of every model answer: placeholders, glossary, key echo, runaway length, identical-to-source; one repair round, then quarantine as `needs-review`.
- Plural translation: `.xcstrings` `variations.plural` (top-level and `%#@var@` substitutions), Android `<plurals>`, gettext `msgid_plural` — one unit per CLDR category the target locale needs, written into the native structure.
- `polygo check`: printf / ICU / i18next / composite-format placeholders, CLDR plural categories, empty/identical/length, untranslated fragments in non-Latin-script targets; `--json`, `--strict`, `--fix`.
- `polygo doctor`: config, files, provider reachability, model pulled — with the fix for each failure.
- `polygo status`, `polygo review` (localhost-only approval page), GitHub Action (`Na5co/polygo/action`).
- Providers: Ollama (default, `qwen3:8b`), any OpenAI-compatible endpoint, Anthropic, deterministic mock.
- Distribution: prebuilt binaries for macOS (arm64, x86_64), Linux (musl, arm64, x86_64), Windows; `install.sh`, Homebrew tap, `cargo binstall`.

### Known limitations

- `.xcstrings` device variations and Android `<string-array>` items are preserved but not translated.
- ICU plural messages inside JSON/ARB strings are translated as a whole; `check` validates their structure but not whether the translation added the categories the locale needs.
- An 8B local model gets some Slavic inflections wrong (live test: 8/8 Polish, 6/8 Russian plural forms); use a larger model for those.
