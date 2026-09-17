# Terminal Trove submission

Form: https://terminaltrove.com/submit/ (fields as of 2026)

**Tool name:** polygo

**One-liner (≤ 100 chars):** Lokalise for one person: translate your app's strings with a local model, git-native.

**Description:**

polygo is a single-binary localization CLI. `polygo init` detects Xcode `.xcstrings`, Android `strings.xml`, i18next JSON, Flutter ARB, gettext `.po` and .NET `.resx` projects; `polygo translate` fills in missing or changed strings using Ollama (default `qwen3:8b`, fully offline) or any OpenAI-compatible / Anthropic endpoint; `polygo check` validates placeholders and CLDR plural categories and exits non-zero for CI. Writers are byte-stable, so a diff shows only the strings that changed. A lockfile tracks what was translated from which source text and never overwrites human edits. No account, no telemetry.

**Category:** Development / Productivity

**Language:** Rust

**License:** MIT

**Repository:** https://github.com/Na5co/polygo

**Install:** `brew install na5co/tap/polygo`, `curl -fsSL https://raw.githubusercontent.com/Na5co/polygo/main/install.sh | sh`, `cargo binstall polygo`

**Screenshot / GIF:** `docs/demo.gif` from the repository (15 s, 900 px wide).

**Platforms:** macOS, Linux, Windows
