![polygo translating an Xcode string catalog with a local model, then checking placeholders](docs/demo.gif)

# polygo

**Lokalise for one person.** A 4 MB CLI that translates your app's strings with a local model, remembers what changed, and refuses to write a translation with a broken placeholder or a missing plural.

[![crates.io](https://img.shields.io/crates/v/polygo)](https://crates.io/crates/polygo) [![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

```sh
curl -fsSL https://raw.githubusercontent.com/Na5co/polygo/main/install.sh | sh
```

<sub>or `brew install na5co/tap/polygo` · `cargo install polygo` · [Windows](https://github.com/Na5co/polygo/releases). Local models need [Ollama](https://ollama.com); or bring an API key.</sub>

## Ten seconds: is my localization broken?

```sh
polygo check Localizable.xcstrings     # or app/src/main/res, or locales/
```

No config, no model, no network. It detects the format and the locales and reports every placeholder that went missing in a translation, every plural form Polish or Arabic needs and doesn't have, every string left half in English. Exit 1 if anything is wrong, so it drops straight into CI.

## Thirty seconds: translate

```sh
cd your-app
polygo init          # finds your string files, writes polygo.toml
polygo use gemma4    # picks a model (pulls it through Ollama, or openai/... with a key)
polygo translate     # new and changed strings, into every target locale
polygo check         # placeholders, plurals, lengths; exit 1 on errors
git diff             # look it over, commit
```

Works with `.xcstrings`, Android `strings.xml`, Flutter ARB, i18next JSON, gettext `.po` and .NET `.resx`. No string files yet? `polygo extract --rewrite` pulls the text out of your web markup and swaps in `t("key")` calls ([how](docs/extract.md)).

## What you get

- **Only the diff you meant.** Writes into your existing files byte for byte. A lockfile records every source string, so editing one English string re-translates one string, and hand edits are never overwritten.
- **Context from your code.** Before translating "Open" it finds `Button("Open")` and tells the model it's a menu item, and attaches similar strings you already translated. In a blind test judged by a second model this won 9 to 5.
- **Plurals done per language.** Polish gets `one`, `few`, `many`, `other`; Japanese gets one form. Written into `.xcstrings` variations, `<plurals>`, `msgstr[n]`.
- **Checks the model can't talk its way past.** Placeholders, CLDR plural sets, empty, identical, half-translated (`ようこそ back!`). Wrong twice and it's quarantined, not written.
- **A second opinion.** `polygo audit` has a different model grade each translation 1 to 5 with a reason. `--fix` redoes the flagged ones.
- **Runs in CI.** `polygo check --strict`, or the [GitHub Action](action/README.md) that opens a PR with new translations and a coverage table.

## Proof

`polygo check` on the DuckDuckGo macOS browser's shipped catalog, unmodified (`git clone` it and run `polygo check DuckDuckGo/Localizable.xcstrings` yourself):

```
error   Localizable.xcstrings  open.in                  [fr]  placeholders: missing %1$@
error   Localizable.xcstrings  permission.popup.title   [it]  placeholders: missing %#@…@
error   Localizable.xcstrings  fire.dialog.history.count [pl] placeholders: missing %#@…@
…
12 error(s), 379 warning(s)
```

Across DuckDuckGo and Ice Cubes: 32 placeholder bugs and 44 missing Slavic plural forms, all in production. Details in [KNOWN_BUGS.md](tests/corpus/xcstrings/KNOWN_BUGS.md).

## Which model

| | Size | Good for |
|---|---|---|
| `qwen3:8b` (default) | 5 GB | UI strings into major languages |
| `gemma4` | 9.6 GB | Smaller languages, Slavic plurals, anything a customer reads |
| `openai/…`, `anthropic/…` | API | The same, without a local GPU |

`polygo models` lists them; `polygo use <model>` switches. Honest numbers and the tradeoffs are in [docs/models.md](docs/models.md).

## Privacy

No telemetry, no account, no server. The only network request is the translation call to the provider in your `polygo.toml`; with Ollama that's `127.0.0.1`. `check`, `status`, `init` and `review` never touch the network. [How to verify it yourself.](docs/README.md#privacy)

## More

[Configuration and commands](docs/README.md) · [Per-format notes](docs/formats/) · [Extracting strings from web apps](docs/extract.md) · [FAQ and comparison with Lokalise, Crowdin, Weblate](docs/faq.md) · [How it was built](docs/dev/GAUNTLET.md)

```sh
cargo test                  # offline, about 10 s
scripts/demo.sh             # re-record the GIF
```

