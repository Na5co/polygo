# awesome-* list PRs

One PR per list, only after the first tagged release exists and only where the list's CONTRIBUTING allows a project this young. One line in alphabetical position, PR title `Add polygo`. At most one PR per day.

## Entry line

Use the list's own bullet style and adapt the tail to the list's topic:

```
- [polygo](https://github.com/Na5co/polygo) - Local-first localization CLI: translates .xcstrings, Android, i18next, ARB, .po and .resx files with a local model (Ollama) or any LLM endpoint; validates placeholders and plurals. MIT.
```

## Targets

| List | Section | Angle for the tail |
|---|---|---|
| rust-unofficial/awesome-rust | Applications › Utilities | Rust, single binary |
| agarrharr/awesome-cli-apps | Development | CLI, offline |
| jpomykala/awesome-i18n | Tools | six formats, placeholder + plural checks |
| matteocrippa/awesome-swift | Localization | `.xcstrings`, Xcode-style writer |
| Solido/awesome-flutter | Internationalization | ARB from `l10n.yaml`, ICU checks |

## PR body

```
Adds polygo, an MIT-licensed localization CLI written in Rust. It translates app
string files (.xcstrings, Android strings.xml, i18next JSON, Flutter ARB, gettext
.po, .NET .resx) with a local Ollama model by default or any OpenAI-compatible /
Anthropic endpoint, and validates placeholders and CLDR plural categories. Writers
are byte-stable and a lockfile prevents overwriting human edits.

README with a demo GIF, per-format docs, CI and a 70-test suite are in the repo.
```
