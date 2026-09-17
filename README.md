![polygo translating an Xcode string catalog with a local model, then checking placeholders](docs/demo.gif)

# polygo

Lokalise for one person. A single-binary CLI that translates your app's strings with a local model, keeps track of what changed in a lockfile, and checks every translation for broken placeholders and missing plurals. It writes into the files you already have, so the diff is just the new strings.

Made for solo developers and small teams shipping an iOS, Android, Flutter, web or .NET app in a few languages. [Lokalise starts at $149/month](https://lokalise.com/pricing) and keeps your strings on its servers. polygo is a 4 MB binary that runs in your repo.

```sh
curl -fsSL https://raw.githubusercontent.com/Na5co/polygo/main/install.sh | sh
```

Also: `brew install na5co/tap/polygo`, `cargo binstall polygo`, `cargo install polygo`, or the [Windows zip](https://github.com/Na5co/polygo/releases). Local models run through [Ollama](https://ollama.com) (`brew install ollama`). If you don't want Ollama, `polygo use openai/gpt-4o-mini --api-key ...` works the same way.

## Quick start

```sh
cd your-app
polygo init          # finds your string files, writes polygo.toml
polygo use gemma4    # picks a model and pulls it (or openai/..., anthropic/... with a key)
polygo doctor        # is everything in place? says what to fix if not
polygo translate     # translates new and changed strings into every target locale
polygo check         # placeholders, plurals, lengths; exit 1 on errors
git diff             # look it over, commit
```

`init` recognises `.xcstrings` catalogs, Android `values-*` folders, Flutter `l10n.yaml` and ARB files, i18next `locales/`, gettext `.po` trees and .NET `.resx`/`.resw`.

## Formats

| Format | Used by | What `init` looks for |
|---|---|---|
| `.xcstrings` | iOS / macOS (Xcode 15+) | one catalog with every locale; plural variations and `%#@var@` substitutions are translated per CLDR category |
| `strings.xml` | Android | `res/values/` plus `res/values-<locale>/`; `<plurals>` translated per quantity |
| `.json` (i18next) | React / web | `locales/<locale>.json` or `locales/<locale>/<ns>.json`, nested keys |
| `.arb` | Flutter | `l10n.yaml` pointing at `lib/l10n/app_<locale>.arb`; ICU plurals and `@metadata` |
| `.po` | gettext (Django, Rails, Python, PHP) | `locale/<locale>/LC_MESSAGES/*.po` or flat `<locale>.po`; `msgid_plural` filled per `Plural-Forms` slot |
| `.resx` / `.resw` | .NET / WinUI | `Name.<locale>.resx` or `<locale>/Resources.resw` |

Every writer round-trips byte for byte: parse then serialize gives back the original file. This is tested on 35 real files from DuckDuckGo, Ice Cubes, Grafana, Django, NewPipe, Penpot and others, so a translation never hides a real change under a reformatting diff.

## Why not paste the file into a chat window

- **It knows what changed.** `polygo.lock` stores a hash of every source string per locale. Change the English and only that string gets translated again. Edit a translation by hand and it is marked `edited` and never overwritten.
- **It reads your code.** Before translating "Open" it finds `Button("Open")` in `LibraryView.swift` and tells the model this is a menu item, not a verb. It also attaches the most similar strings you already translated so terms stay consistent. In a blind comparison judged by a second model, this context won 9 to 5 with the rest tied.
- **Plurals are done properly.** "%lld photos" becomes four strings for Polish (one, few, many, other) and one for Japanese. Each form is requested with a concrete count so the model picks the right inflection, and written into the format's own structure.
- **It checks the output.** Placeholders (`%@`, `%1$s`, `{count}`, `{{name}}`, `%(name)s`, `{0}`, ICU plural blocks) have to survive translation. CLDR plural categories have to be complete for the locale. Empty, identical, oversized and half-translated strings (`ようこそ back!`) are flagged. Anything the model gets wrong twice is quarantined as `needs-review` and not written to your files.
- **A second model can grade the first.** `polygo audit --locale bg` asks a different model (gemma4 by default, or any API model) to score each translation 1 to 5 with a reason. Structural checks can't see a wrong word; this can. `--fix` re-translates the flagged ones and leaves human edits alone.
- **It remembers.** Translations that a human wrote or approved go into `~/.config/polygo/memory.toml`. The next project that has "Cancel" gets it back without a model call. Model output is only reused as examples. `polygo memory` shows what is stored.
- **It runs in CI.** `polygo check --json --strict` in a pipeline, or the [GitHub Action](action/README.md) that opens a PR with new translations on every push, coverage table included.
- **Review is local.** `polygo review` serves a page on `127.0.0.1` to approve or reject pending translations. Approvals are recorded as human.
- **Pseudo-localization.** `polygo pseudo` writes `[Šáṽé çĥáñĝéš ~~~~]` for every string as `en-XA`. Run the app in that locale to spot hardcoded text and truncation.
- **Per-key control from the code.** `polygo:skip` and `polygo:max=20` in a developer comment are respected by translate and check.

Running `polygo check` on public repos found 32 shipped placeholder bugs in the DuckDuckGo macOS browser and Ice Cubes (`Ouvrir dans % @`, a dropped `%d`, broken `%#@var@` variables), 44 missing Slavic plural forms in Ice Cubes, and a Ukrainian string that reads "ключа DeepL API key". Details in [`tests/corpus/xcstrings/KNOWN_BUGS.md`](tests/corpus/xcstrings/KNOWN_BUGS.md).

## Which model

`polygo models` lists the options with sizes and notes. The default `qwen3:8b` (5 GB) is fine for UI strings into major languages. In a live test it got 8 of 8 Polish plural forms and 6 of 8 Russian, and its Bulgarian was rough ("Достъп за достъп"). `check` catches structural mistakes, not a wrong word, so for smaller languages, Slavic plurals or anything customer-facing use `polygo use gemma4` (9.6 GB) or an API model. The workflow is the same either way.

```sh
polygo models                                   # what is pulled, what is active, what each is good at
polygo use gemma4                               # pulls through Ollama with a progress bar, writes polygo.toml
polygo use qwen3:14b --global                   # also the default for future `polygo init`
polygo use openai/gpt-4o-mini --api-key sk-...  # key stored in ~/.config/polygo/credentials.toml (0600)
polygo use anthropic/claude-sonnet-5            # or export ANTHROPIC_API_KEY
polygo use openai/llama-3.3-70b --base-url https://api.groq.com/openai/v1   # any OpenAI-compatible server
```

## Offline

The default provider is Ollama on `127.0.0.1:11434`. The GIF above was recorded with `qwen3:8b` on a MacBook. The test suite makes no network calls, and `check`, `status`, `review` and `init` never use the network. You can verify this on macOS by blocking every connection except the local Ollama port:

```sh
cat > offline.sb <<'SB'
(version 1) (allow default) (deny network*) (allow network* (remote ip "localhost:11434"))
SB
sandbox-exec -f offline.sb polygo translate
# translated 2 string(s) in 1 batch(es) with ollama (qwen3:8b)
```

`polygo translate --dry-run` lists what would be sent and sends nothing.

To use an API model instead, `polygo use ...` writes this for you:

```toml
# polygo.toml
[provider]
kind = "openai"                          # or "anthropic", or "ollama"
model = "gpt-4o-mini"
base_url = "https://api.openai.com/v1"   # any OpenAI-compatible server: llama.cpp, vLLM, LM Studio, OpenRouter
```

API keys come from `POLYGO_API_KEY`, `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`, or `~/.config/polygo/credentials.toml` (written by `polygo use --api-key`, mode 0600; the environment wins). They are only sent to the `base_url` you configured.

## Security and privacy

- No telemetry, no analytics, no update checks. The only network request polygo makes is the translation call to the provider in your `polygo.toml`. With the default Ollama provider that is localhost.
- `polygo review` binds to `127.0.0.1` only and rejects requests whose `Host` header is not localhost.
- Nothing is written outside your repo (`polygo.toml`, `polygo.lock`, your string files) except `~/.config/polygo/` when you ask for it with `polygo use --global` or `--api-key`.
- One static binary, about 4 MB, no runtime dependencies. Built on GitHub Actions; every release ships a `SHA256SUMS`.

## Configuration

```toml
source_locale = "en"
target_locales = ["de", "fr", "ja"]
batch_size = 20        # strings per model call
jobs = 1               # parallel model calls
context = true         # attach code usage and similar translations to prompts
memory = true          # use the cross-project translation memory
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

`glossary.toml` pins brand names and required translations:

```toml
do_not_translate = ["Polygo", "GitHub"]

[terms.de]
"Sign in" = "Anmelden"
```

## Commands

| | |
|---|---|
| `polygo init` | detect project type and locales, write `polygo.toml` |
| `polygo models` | local models with sizes and notes, marks pulled (`+`) and active (`*`), API options |
| `polygo use <model> [--base-url] [--api-key] [--global] [--no-pull]` | pull an Ollama model or set an API provider, writes `[provider]` |
| `polygo doctor [--json]` | config parses, files load, provider reachable, model pulled; each failure names its fix |
| `polygo translate [--locale de,fr] [--dry-run] [-v] [--retry-review] [--no-context]` | translate new and changed strings; exit 3 if some were quarantined |
| `polygo check [--json] [--strict] [--fix]` | placeholders, plurals, lengths, untranslated fragments; `--fix` re-translates the failures |
| `polygo audit [--locale] [--judge gemma4] [--threshold 3] [--fix] [--json]` | a second model grades translations 1 to 5 with reasons; exit 1 when anything is flagged |
| `polygo pseudo [--locale en-XA]` | write a pseudo-locale to catch hardcoded strings and truncation |
| `polygo memory [--forget]` | cross-project translation memory |
| `polygo status [--json] [--markdown]` | counts per locale; `--markdown` is a coverage table for a README or PR |
| `polygo review [--port 4133] [--open]` | local page to approve or reject quarantined translations |

Per-format notes are in [`docs/`](docs/).

## Development

```sh
cargo test                  # offline, about 10 s
cargo test --features live  # also hits a local Ollama
scripts/killtest.sh tests/corpus/xcstrings/loop.xcstrings de   # blind A/B judged by a second model
scripts/demo.sh             # re-record docs/demo.gif (vhs + ffmpeg)
```

MIT, Atanas Angelov
