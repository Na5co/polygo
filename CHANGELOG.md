# Changelog

All notable changes to polygo. Versions follow [SemVer](https://semver.org); dates are ISO.

## 0.1.0 — unreleased

First release.

- Formats: Xcode `.xcstrings`, Android `strings.xml`, i18next JSON, Flutter ARB, gettext `.po`, .NET `.resx`/`.resw`. Every writer is byte-stable (35 real-world files in `tests/corpus` round-trip exactly).
- `polygo init` detects the project layout and locales for all six formats.
- `polygo translate`: lockfile-driven incremental translation, batches, parallel jobs, crash-safe resume, `--dry-run`, `--retry-review`, `--no-context`. Human-edited translations are never overwritten.
- Context retrieval: code-usage snippets (Aho-Corasick over the source tree) and similar already-translated strings, within a token budget.
- Validation and repair of every model answer: placeholders, glossary, key echo, runaway length, identical-to-source; one repair round, then quarantine as `needs-review`.
- `polygo check`: printf / ICU / i18next / composite-format placeholders, CLDR plural categories, empty/identical/length; `--json`, `--strict`, `--fix`.
- `polygo status`, `polygo review` (localhost-only approval page), GitHub Action (`atanasa/polygo/action`).
- Providers: Ollama (default, `qwen3:8b`), any OpenAI-compatible endpoint, Anthropic, deterministic mock.
- Distribution: prebuilt binaries for macOS (arm64, x86_64), Linux (musl, arm64, x86_64), Windows; `install.sh`, Homebrew tap, `cargo binstall`.

### Known limitations

- `.xcstrings` plural/device variations, Android `<plurals>`/`<string-array>` and gettext `msgid_plural` entries are preserved but not translated yet; `check` validates them.
