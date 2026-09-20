# FAQ

**Does it work offline?** Yes. The default provider is Ollama on localhost; `check`, `status`, `review` and `init` never use the network at all. See [Privacy](#privacy) for how to prove it with `sandbox-exec`.

**Will it overwrite translations I edited by hand?** No. The lockfile marks them `edited`; `translate` and `audit --fix` skip them. The one exception is `check --fix`, which re-translates a hand-edited string only when it has a structural error such as a missing placeholder.

**Can I use OpenAI, Anthropic, Groq, OpenRouter or my own llama.cpp / vLLM / LM Studio server?** Yes: `polygo use openai/<model> --base-url <url> --api-key <key>`, or `polygo use anthropic/<model>`. Any OpenAI-compatible endpoint works.

**Does it send my strings anywhere?** Only to the provider in your `polygo.toml`, and only the strings that need translating (`translate --dry-run` shows exactly which). With Ollama that is `127.0.0.1`.

**Is it free?** MIT, no account, no tier. Ollama models cost nothing; API models cost whatever your provider charges.

**The model is bad at my language.** Switch to `gemma4` or an API model (`polygo use`), run `polygo audit` to have a second model grade the output, and approve or reject the flagged ones in `polygo review`.

**My app has no string files yet.** Web: `polygo extract` builds `locales/en.json` from your markup. iOS, Android, Flutter and gettext: use the platform's own extractor, then `polygo init`.

**Does it run in CI?** `polygo check --json --strict` fails the build on broken placeholders; the [GitHub Action](action/README.md) translates on push and opens a PR.

## Compared with other tools

| | |
|---|---|
| **Lokalise, Crowdin, Phrase** | Hosted translation platforms: web editor, translator accounts, subscription pricing, your strings on their servers. polygo has no server and no accounts; the review step is `git diff`. |
| **Weblate** | Open-source web platform you host yourself (database, web UI, workers). polygo is one binary in the repo with nothing to run. |
| **Editor extensions (i18n Ally and friends)** | Show and edit keys inline while you code. polygo translates in batches, checks in CI and keeps a lockfile; the two sit side by side fine. |
| **A DeepL or Google Translate script** | One string at a time, no code context, no CLDR plural forms, no placeholder check. That gap is what `check`, `audit` and the context engine close. |
| **Pasting the file into ChatGPT** | See the section above. |

polygo is MIT-licensed and open source. There is no hosted tier, no account and no telemetry.
