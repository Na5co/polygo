# r/reactjs

**Title:** Open-source CLI that fills in i18next locales/*.json with a local model and checks {{placeholders}} and _one/_other plural keys

**Body:**

For anyone with `public/locales/en/translation.json` and a few half-finished sibling files:

`polygo init` detects the i18next layout (flat `locales/de.json` or namespaced `locales/de/common.json`), `polygo translate` writes the missing keys, and `polygo check` fails when a `{{name}}` or `$t(other.key)` got mangled, or when a `key_one`/`key_other` group is missing a category that language needs.

Why not just paste the JSON into a chat:

- It writes back with your file's indentation and key order — the diff is only the new strings.
- It greps your components for `t('settings.title')` and shows the model the surrounding JSX, plus similar strings you already translated, so "Save" as a button and "save" in a sentence come out differently.
- A lockfile records which source strings changed, so re-running after editing English only re-translates those. Hand edits are never overwritten.
- Default model is `qwen3:8b` on Ollama, so it runs offline with no API key. Any OpenAI-compatible endpoint (or Anthropic) works too.
- `polygo check --json --strict` is meant for CI; there's a GitHub Action that opens a PR.

Round-trip tested on Excalidraw's, Hoppscotch's, Grafana's and Immich's locale files.

Rust, one ~4 MB binary, MIT, no telemetry: https://github.com/atanasa/polygo

Question: do most of you keep one big `translation.json` or split namespaces per feature? `init` handles both — which one should the docs lead with?
