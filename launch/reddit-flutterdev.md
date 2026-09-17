# r/FlutterDev

**Title:** Open-source CLI that fills in your app_*.arb files with a local model and validates the ICU plurals — no account, no API key

**Body:**

If your `l10n.yaml` points at `lib/l10n/app_en.arb` and you've been hand-copying keys into `app_de.arb`, this might save you an afternoon.

`polygo init` reads `l10n.yaml`, `polygo translate` writes every missing key into each locale's ARB in the template's key order (what `flutter gen-l10n` expects), and `polygo check` fails when a `{count, plural, …}` message lost a branch, a `{name}` placeholder was renamed, or braces don't balance.

What it does differently from pasting into a chat:

- `@key` descriptions and placeholder names from the template go into the prompt, plus the Dart code where the key is used and similar strings you already translated.
- Only changed source strings are re-translated (lockfile); your hand edits are never overwritten.
- Existing ARB files keep their formatting byte-for-byte; the diff is just the new strings.
- Default provider is `qwen3:8b` on Ollama, so it works offline. Any OpenAI-compatible or Anthropic endpoint also works.

Tested on FluffyChat's, Ente's and Immich's ARB bundles for round-trip stability. Rust, single ~4 MB binary, MIT.

https://github.com/Na5co/polygo

One thing I haven't decided: should `check` insist that a Polish translation adds `few`/`many` branches to every plural message, or is that too strict for how people actually write ICU in ARB — especially if you localise into Slavic languages?
