# r/androiddev

**Title:** A small CLI that translates res/values/strings.xml into values-* with a local model and checks every %1$s — free, MIT

**Body:**

I built this for my own side project and it might be useful here.

`polygo init` finds `res/values/strings.xml` and your existing `values-de`, `values-pt-rBR`… folders. `polygo translate` writes the missing strings into each of them. `polygo check` fails when a translation drops or duplicates a `%1$s`/`%d`, and when a `<plurals>` block lacks a quantity the locale needs (`few`/`many` for Polish and Russian — the usual crash-at-runtime kind).

Details that mattered to me:

- Byte-stable writer: it splices values into your existing XML, so comments, CDATA, `translatable="false"` and formatting are untouched and the diff is only the new strings.
- It reads the Kotlin/Java/XML where a string is referenced so the model knows whether "Clear" is a button or a weather condition, and attaches similar strings you already translated.
- A lockfile tracks what changed in the source; hand-edited translations are never overwritten.
- Default model is `qwen3:8b` on Ollama (offline, no API key). OpenAI-compatible and Anthropic endpoints work too.
- There's a GitHub Action that opens a PR with new translations on every push.

Round-trip tested against AntennaPod, NewPipe, F-Droid, Thunderbird and DuckDuckGo's Android catalogs.

Not yet: `<plurals>` and `<string-array>` items are validated but not translated. That's the next thing.

Single Rust binary, MIT, no telemetry: https://github.com/atanasa/polygo

Genuine question: for those using Crowdin/Weblate with community translators, is there anything a local tool could do that would fit *alongside* that, rather than replace it?
