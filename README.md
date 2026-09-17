![polygo translating an Xcode string catalog with a local model, then checking placeholders](docs/demo.gif)

# polygo

**Lokalise for one person.** A single-binary, git-native localization CLI that translates your app's strings with a local model (Ollama by default — no API key, no account, nothing leaves your machine), keeps track of what changed in a lockfile, and checks every translation for broken placeholders and missing plurals.

Built for solo developers and small teams who ship an iOS, Android, Flutter, web or .NET app in a few languages and don't want a translation-management SaaS in the loop. [Lokalise starts at $149/month](https://lokalise.com/pricing) and wants your strings on its servers; polygo is a 4 MB binary you run in your repo.

```sh
curl -fsSL https://raw.githubusercontent.com/atanasa/polygo/main/install.sh | sh
```

Also: `brew install atanasa/tap/polygo` · `cargo binstall polygo` · `cargo install polygo` · [Windows zip on the releases page](https://github.com/atanasa/polygo/releases). Requires [Ollama](https://ollama.com) with `ollama pull qwen3:8b` for the default provider (~5 GB, runs on a laptop), or point it at any OpenAI-compatible or Anthropic endpoint.

## Quick start

```sh
cd your-app
polygo init          # detects the project type and locales → writes polygo.toml
polygo translate     # translates new/changed strings into every target locale
polygo check         # placeholders, plurals, lengths; exit 1 on errors
git diff             # review, commit, done
```

`init` finds `.xcstrings` catalogs, Android `values-*` dirs, Flutter `l10n.yaml`/ARB, i18next `locales/`, gettext `.po` trees and .NET `.resx`/`.resw`. `translate` writes into your existing files without reformatting them — the diff shows only the strings that changed.

## Formats

| Format | Ecosystem | Layout `init` detects |
|---|---|---|
| `.xcstrings` | iOS / macOS (Xcode 15+ string catalogs) | one catalog holding every locale, incl. plural variations |
| `strings.xml` | Android | `res/values/` + `res/values-<locale>/`, plurals and string arrays |
| `.json` (i18next) | React / web | `locales/<locale>.json` or `locales/<locale>/<ns>.json`, nested keys |
| `.arb` | Flutter | `l10n.yaml` → `lib/l10n/app_<locale>.arb`, ICU plurals and `@metadata` |
| `.po` | gettext (Django, Rails, Python, PHP…) | `locale/<locale>/LC_MESSAGES/*.po` or flat `<locale>.po`, plurals and `msgctxt` |
| `.resx` / `.resw` | .NET / WinUI | `Name.<locale>.resx` or `<locale>/Resources.resw` |

Every writer is byte-stable: parse → serialize reproduces the original file exactly (tested on 35 real files from open-source apps such as DuckDuckGo, IceCubes, Grafana, Django, NewPipe and Penpot), so translations never bury a real change under a reformatting diff.

## What it does that a chat window doesn't

- **Knows what changed.** `polygo.lock` records a hash of every source string per locale. Edit the English, and only that string is re-translated. Hand-edit a translation and it is marked `edited` and never overwritten.
- **Reads your code.** Before translating `"Open"` it finds `Button("Open")` in `LibraryView.swift` and tells the model this is a menu item, not a verb in a sentence. It also attaches the most similar strings you already translated, so terminology stays consistent. In blind A/B judging with a second model, context won 9 : 5 with the rest tied.
- **Checks what the model produced.** Placeholders (`%@`, `%1$s`, `{count}`, `{{name}}`, `%(name)s`, `{0}`, ICU `{count, plural, …}`) must survive translation; CLDR plural categories must be complete for the locale; empty, untranslated and runaway-length strings are flagged. Anything the model gets wrong twice is quarantined as `needs-review` and never written to your files.
- **Runs in CI.** `polygo check --json --strict` in a pipeline, or the [GitHub Action](action/README.md) that opens a PR with new translations on every push.
- **Reviews locally.** `polygo review` serves a page on `127.0.0.1` to approve or reject pending translations; approvals are recorded as human.

Run against real projects, `polygo check` found 32 shipped placeholder bugs in the DuckDuckGo macOS browser and IceCubes (`Ouvrir dans % @`, a dropped `%d`, mangled `%#@var@` variables) and 44 missing Slavic plural forms in IceCubes — see [`tests/corpus/xcstrings/KNOWN_BUGS.md`](tests/corpus/xcstrings/KNOWN_BUGS.md).

## Runs fully offline

The default provider is Ollama on `127.0.0.1:11434`; the GIF above was recorded with `qwen3:8b` on a MacBook. The test-suite (`cargo test`, 69 tests) makes no network calls, and `check`, `status`, `review` and `init` never touch the network at all. Proof you can run yourself on macOS — deny every network connection except the local Ollama port and translate anyway:

```sh
cat > offline.sb <<'SB'
(version 1) (allow default) (deny network*) (allow network* (remote ip "localhost:11434"))
SB
sandbox-exec -f offline.sb polygo translate
# translated 2 string(s) in 1 batch(es) with ollama (qwen3:8b)
```

`polygo translate --dry-run` lists what would be sent and sends nothing.

Bring your own model when you want a bigger one:

```toml
# polygo.toml
[provider]
kind = "openai"                      # or "anthropic", or "ollama"
model = "gpt-4o-mini"
base_url = "https://api.openai.com/v1"   # any OpenAI-compatible server: llama.cpp, vLLM, LM Studio, OpenRouter…
```

API keys are read from `POLYGO_API_KEY`, `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` and are only sent to the `base_url` you configured.

## Security & privacy

- **No telemetry, no analytics, no update checks.** polygo makes exactly one kind of network request: the translation call to the provider in your `polygo.toml`. With the default Ollama provider that is localhost.
- **`polygo review` binds to `127.0.0.1` only** and rejects requests whose `Host` header is not localhost (DNS-rebinding guard).
- **Nothing is written outside your repo**: `polygo.toml`, `polygo.lock` and your existing localization files.
- Single static binary, ~4 MB, no runtime dependencies. Built from source on GitHub Actions; every release ships a `SHA256SUMS`.

## Configuration

```toml
source_locale = "en"
target_locales = ["de", "fr", "ja"]
batch_size = 20        # strings per model call
jobs = 1               # parallel model calls
context = true         # attach code usage + similar translations to prompts
glossary = "glossary.toml"

[[files]]
format = "android"
path = "app/src/main/res/values/strings.xml"
locale_path = "app/src/main/res/values-{android_locale}/strings.xml"

[provider]
kind = "ollama"
model = "qwen3:8b"
timeout_secs = 300
```

`glossary.toml` pins brand terms and required translations:

```toml
do_not_translate = ["Polygo", "GitHub"]

[terms.de]
"Sign in" = "Anmelden"
```

## Commands

| | |
|---|---|
| `polygo init` | detect project type and locales, write `polygo.toml` |
| `polygo translate [--locale de,fr] [--dry-run] [-v] [--retry-review] [--no-context]` | translate new / changed strings; exit 3 if some were quarantined |
| `polygo check [--json] [--strict] [--fix]` | validate placeholders, plurals and lengths; `--fix` re-translates the failures |
| `polygo status [--json]` | new / stale / untranslated / edited / needs-review counts per locale |
| `polygo review [--port 4133] [--open]` | local page to approve or reject quarantined translations |

Full per-format notes live in [`docs/`](docs/).

## Development

```sh
cargo test                  # offline, ~10 s
cargo test --features live  # additionally hits a local Ollama
scripts/killtest.sh tests/corpus/xcstrings/loop.xcstrings de   # blind A/B vs a second model
scripts/demo.sh             # re-record docs/demo.gif (vhs + ffmpeg)
```

MIT © Atanas Angelov
