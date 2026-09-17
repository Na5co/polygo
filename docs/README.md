# polygo docs

- [Per-format notes](#formats) — what `init` detects, how plurals and placeholders are handled, what is preserved byte-for-byte.
- [`polygo.toml` reference](#polygotoml)
- [Lockfile](#polygolock) — how polygo knows what changed.
- [Providers](#providers) — Ollama (default), OpenAI-compatible, Anthropic.
- [CI](../action/README.md) — the GitHub Action.
- [Re-recording the demo](../scripts/demo.sh)

## Formats

| Page | Ecosystem |
|---|---|
| [formats/xcstrings.md](formats/xcstrings.md) | iOS / macOS string catalogs |
| [formats/android.md](formats/android.md) | Android `strings.xml` |
| [formats/json.md](formats/json.md) | i18next / React / web JSON |
| [formats/arb.md](formats/arb.md) | Flutter ARB |
| [formats/po.md](formats/po.md) | gettext `.po` |
| [formats/resx.md](formats/resx.md) | .NET `.resx` / WinUI `.resw` |

## polygo.toml

```toml
source_locale = "en"
target_locales = ["de", "fr", "ja"]
batch_size = 20          # strings per model call (smaller = more context per string, slower)
jobs = 1                 # parallel model calls; raise for API providers
length_ratio = 2.5       # `check` warns when a translation is longer than this × source (+8 chars)
context = true           # attach code usage + similar translations to prompts
context_tokens = 600     # approximate context budget per string
glossary = "glossary.toml"

[[files]]                # repeat per file
format = "android"       # xcstrings | android | json | arb | po | resx
path = "app/src/main/res/values/strings.xml"
locale_path = "app/src/main/res/values-{android_locale}/strings.xml"

[provider]
kind = "ollama"          # ollama | openai | anthropic | mock
model = "qwen3:8b"
base_url = "http://127.0.0.1:11434"   # optional
timeout_secs = 300
```

`locale_path` templates accept `{locale}` (as written in `target_locales`, e.g. `pt-BR`) and `{android_locale}` (Android resource qualifier, `pt-rBR`). `xcstrings` has no `locale_path` — one catalog holds every locale.

When several `[[files]]` are configured, lockfile keys are prefixed with the file path (`app/src/main/res/values/strings.xml:welcome`).

`glossary.toml`:

```toml
do_not_translate = ["Polygo", "GitHub"]   # must appear verbatim in every translation

[terms.de]
"Sign in" = "Anmelden"                    # required rendering of a term
```

A translation that violates the glossary is sent back once for repair, then quarantined.

## polygo.lock

A sorted TOML file, meant to be committed. For every key × locale it stores the blake3 hash of the source text the translation was made from, the provider and model, a timestamp, and — for quarantined strings — the reason.

States shown by `polygo status`:

| state | meaning |
|---|---|
| `new` | key exists in the source file, never seen by polygo |
| `stale` | source text changed since the translation was made |
| `untranslated` | key known, no translation in the locale file |
| `edited` | the translation in the file differs from what polygo wrote (a human edited it, or it pre-dates polygo); never overwritten |
| `needs-review` | quarantined after the model failed validation twice; not written to your files; retried only with `--retry-review` or resolved in `polygo review` |
| `up-to-date` | translation matches the current source |

Delete `polygo.lock` to re-translate everything except `edited` strings (pre-existing translations are treated as human).

## Providers

| kind | endpoint | key |
|---|---|---|
| `ollama` (default) | `$OLLAMA_HOST` or `http://127.0.0.1:11434`, `/api/chat` with a JSON schema, thinking off | none |
| `openai` | `base_url` + `/chat/completions` — works with llama.cpp, vLLM, LM Studio, OpenRouter, Groq… | `POLYGO_API_KEY` or `OPENAI_API_KEY` |
| `anthropic` | `base_url` + `/v1/messages` | `POLYGO_API_KEY` or `ANTHROPIC_API_KEY` |
| `mock` | none — deterministic `⟦de⟧ Source` output for tests and dry runs of your pipeline | none |

Every provider gets the same prompt: the source string, its key, the developer comment, where the string is used in code (a few lines around each hit), up to three similar strings you already translated into that locale, and the glossary. The model answers one JSON object per batch. Each answer is validated (every key present, placeholders intact, glossary respected, not identical to the source, not a runaway) and repaired once before the string is quarantined.

`POLYGO_DEBUG_PROMPT=1` prints every prompt and raw response to stderr.
