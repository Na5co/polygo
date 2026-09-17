# r/iOSProgramming

**Title:** I made a CLI that translates .xcstrings catalogs with a local model and checks every %@ and plural — free, MIT

**Body:**

I got tired of choosing between a $149/month translation SaaS and pasting my string catalog into a chat window, so I wrote a small Rust CLI for it.

`polygo init` finds your `.xcstrings`, `polygo translate` fills in the missing locales, `polygo check` fails if a `%@`, `%lld` or `%#@var@` went missing or a locale lacks a plural category Xcode will complain about at runtime.

Things I cared about:

- It writes back in Xcode's exact style (two-space indent, `"key" : value`, Xcode's key order), so `git diff` shows only the strings that changed.
- It reads your Swift code: before translating "Open" it finds `Button("Open")` and tells the model it's a menu item, and it attaches similar strings you already translated so terminology stays consistent.
- Human edits are never overwritten — a lockfile tracks what it wrote.
- Default model is `qwen3:8b` on Ollama, so nothing leaves the machine. Any OpenAI-compatible or Anthropic endpoint works if you prefer a bigger model.

Running `polygo check` on public catalogs was eye-opening: DuckDuckGo's macOS browser ships `Ouvrir dans % @` (space inside the placeholder) in five locales, and Ice Cubes is missing the `few`/`many` forms in Polish and Ukrainian for 44 strings.

Plural variations (including `%#@var@` substitutions) are translated one CLDR category at a time, so Polish gets its `few`/`many` forms and Japanese just `other`. Device variations are left alone for now.

Free, MIT, no account, no telemetry: https://github.com/atanasa/polygo

Question for people shipping in more languages than me: do you keep translations in the catalog and let Xcode manage state, or export `.xliff` and round-trip — which of the two should the docs lead with?
