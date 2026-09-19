# polygo docs

- [Per-format notes](#formats): what `init` detects, how plurals and placeholders are handled, what is preserved byte-for-byte.
- [`polygo.toml` reference](#polygotoml)
- [Lockfile](#polygolock): how polygo knows what changed.
- [Providers](#providers): Ollama (default), OpenAI-compatible, Anthropic.
- `polygo models` / `polygo use <model>`: pick a model; Ollama models are pulled for you, API keys can be stored (`--api-key`), `--global` sets the default for new projects. User-level state lives in `~/.config/polygo/` (`defaults.toml`, `credentials.toml` mode 0600) or `$POLYGO_CONFIG_DIR`.
- `polygo add <locale>...` / `polygo remove <locale>...`: edit `target_locales` in `polygo.toml` without touching files.
- `polygo extract`: pull UI text out of web markup (JSX/TSX, HTML in template literals, `.html`/`.vue`/`.svelte`) into `locales/en.json`, with file:line for every string. For projects that have no string files yet.
- `polygo doctor`: run it first: it checks the config, loads every file, pings the provider and confirms the model is pulled, printing the fix for whatever fails.
- [CI](../action/README.md): the GitHub Action.
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

[keys]
skip = ["debug.*", "internal_*"]   # never translated, counted or checked (globs over the key)

[provider]
kind = "ollama"          # ollama | openai | anthropic | mock
model = "qwen3:8b"
base_url = "http://127.0.0.1:11434"   # optional
timeout_secs = 300
```

`locale_path` templates accept `{locale}` (as written in `target_locales`, e.g. `pt-BR`) and `{android_locale}` (Android resource qualifier, `pt-rBR`). `xcstrings` has no `locale_path`: one catalog holds every locale.

When several `[[files]]` are configured, lockfile keys are prefixed with the file path (`app/src/main/res/values/strings.xml:welcome`).

`[keys] skip` takes globs over the key (`*` also crosses dots, so `debug.*` covers `debug.net.trace`); with several `[[files]]`, `locales/en/admin.json:*` targets one file. A skipped plural group takes all its forms with it. `polygo status` says how many keys the patterns removed. For formats with a comment field there is also the per-key directive below.

Developer comments can carry per-key directives: `polygo:skip` (never translate, not counted), `polygo:max=20` (translations longer than 20 characters are a `check` error and are bounced back to the model), `polygo:context=...` (plain text for the model: the whole comment is sent anyway). They work in every format that has a comment field (`.xcstrings` comment, `<!-- -->` before an Android element, `@key.description` in ARB, `#.` in .po, `<comment>` in .resx).

`glossary.toml`:

```toml
do_not_translate = ["Polygo", "GitHub"]   # must appear verbatim in every translation

[terms.de]
"Sign in" = "Anmelden"                    # required rendering of a term
```

A translation that violates the glossary is sent back once for repair, then quarantined.

## polygo.lock

A sorted TOML file, meant to be committed. For every key × locale it stores the blake3 hash of the source text the translation was made from, the provider and model, a timestamp, and: for quarantined strings: the reason.

States shown by `polygo status`:

| state | meaning |
|---|---|
| `new` | key exists in the source file, never seen by polygo |
| `stale` | source text changed since the translation was made |
| `untranslated` | key known, no translation in the locale file |
| `edited` | the translation in the file differs from what polygo wrote (a human edited it, or it pre-dates polygo); never overwritten |
| `needs-review` | quarantined after the model failed validation twice; not written to your files; retried only with `--retry-review` or resolved in `polygo review` |

Plural forms appear as `key#plural.<category>` and count only for the locales that need that category.
| `up-to-date` | translation matches the current source |

Delete `polygo.lock` to re-translate everything except `edited` strings (pre-existing translations are treated as human).

## Translation memory

`~/.config/polygo/memory.toml` (or `$POLYGO_CONFIG_DIR/memory.toml`) holds `locale → source → {text, by}`. polygo learns from three places: translations it finds already hand-written in your files (`edited` state), approvals in `polygo review`, and its own model output. Only `by = "human"` entries are reused verbatim: an identical source string in another project is filled without a model call and recorded with `provider = "memory"`. Model entries feed the few-shot examples. `memory = false` in `polygo.toml` opts a project out; `polygo memory --forget` clears it.

## Audit

`polygo audit` sends every existing translation (source, translation, developer comment, where it is used in code) to a judge model and asks for a 1-5 score with a one-line reason. Default judge is `gemma4`; `--judge anthropic/claude-sonnet-5` or any `polygo use` spec works. Strings scored at or below `--threshold` (3) are printed and make the command exit 1; `--json` for pipelines; `--fix` re-translates them with the judge model, skipping anything a human edited. Use it after a big local-model run, or in CI against a small `--limit` sample.

## Pseudo-localization

`polygo pseudo` writes `en-XA` (Android's pseudolocale; `--locale qps-ploc` for .NET) with every string accented, ~40 % longer and bracketed, placeholders and markup untouched. It is not a target locale and not recorded in the lockfile: delete the file or catalog entry when done.

## Providers

| kind | endpoint | key |
|---|---|---|
| `ollama` (default) | `$OLLAMA_HOST` or `http://127.0.0.1:11434`, `/api/chat` with a JSON schema, thinking off | none |
| `openai` | `base_url` + `/chat/completions`: works with llama.cpp, vLLM, LM Studio, OpenRouter, Groq... | `POLYGO_API_KEY` or `OPENAI_API_KEY` |
| `anthropic` | `base_url` + `/v1/messages` | `POLYGO_API_KEY` or `ANTHROPIC_API_KEY` |
| `mock` | none: deterministic `⟦de⟧ Source` output for tests and dry runs of your pipeline | none |

Keys: environment first (`POLYGO_API_KEY`, then the provider's variable), then `~/.config/polygo/credentials.toml`.

Every provider gets the same prompt: the source string, its key, the developer comment, where the string is used in code (a few lines around each hit), up to three similar strings you already translated into that locale, and the glossary. The model answers one JSON object per batch. Each answer is validated (every key present, placeholders intact, glossary respected, not identical to the source, not a runaway, no source words left untranslated in a non-Latin-script target) and repaired once before the string is quarantined.

`POLYGO_DEBUG_PROMPT=1` prints every prompt and raw response to stderr.

### Tracing with Phoenix

Set `PHOENIX_COLLECTOR_ENDPOINT` and every batch is exported as a trace to
[Arize Phoenix](https://github.com/Arize-ai/phoenix) (or any OTLP/HTTP collector):
one span per batch with its input strings and outcome, and one `LLM` span per model
call with the system and user prompt, the raw reply, token counts, latency and errors.
Repair rounds and echo retries show up as their own spans, so you can see which strings
cost a second call and why. All batches of one run share a session.

```sh
docker run -p 6006:6006 arizephoenix/phoenix:latest   # or: pip install arize-phoenix && phoenix serve
PHOENIX_COLLECTOR_ENDPOINT=http://localhost:6006 polygo translate
```

`PHOENIX_PROJECT_NAME` picks the project (default `polygo`); `PHOENIX_API_KEY` is sent
as a bearer token for a hosted or auth-enabled instance. Off when the endpoint is unset;
an unreachable collector is reported once and never fails a run. Prompts are sent as-is,
so point it only at a collector you trust with your strings.

## How it was built

[`docs/dev/GAUNTLET.md`](dev/GAUNTLET.md) is the build log: every feature as a gate with an acceptance command, the A/B and kill-test results, and what broke along the way. `bench/` holds the A/B and kill-test data.
