# Show HN

## Title (80 chars max)

Show HN: Polygo, a local-first CLI that translates your app's strings with Ollama

## URL

https://github.com/Na5co/polygo

## First comment (post right after submitting)

Hi HN, author here.

I ship a small Mac app in five languages. Lokalise starts at $149/month and wants my strings on its servers. The free alternative is pasting a string catalog into a chat window and merging the answer by hand. Neither felt right for a one-person project, so I built the thing I wanted:

- one static Rust binary: `polygo init && polygo translate`
- reads .xcstrings, Android strings.xml, i18next JSON, Flutter ARB, gettext .po, .resx
- writes back into your existing files byte for byte, so the diff is only the strings that changed
- `polygo use gemma4` pulls a model through Ollama and it runs with no API key and no network; any OpenAI-compatible or Anthropic endpoint works too
- a lockfile tracks which source strings changed, so it re-translates only those and never overwrites a human edit

The part I like most: before translating "Open" it greps your code for `Button("Open")` and tells the model it is a menu item. In a blind A/B judged by a different model, that context won 9 to 5.

It checks placeholders and CLDR plural categories. I ran that check over 23 popular open source apps and found about a thousand broken placeholders in production, mostly a translator translating the placeholder name ({{ modelli }} for {{ models }}). Fixes are merged in Open WebUI and Dashy, with more open in AppFlowy, Cal.diy, Umami, DuckDuckGo and Ice Cubes. Since a structural check can't see a wrong word, polygo audit has a second model grade every translation with a reason.

MIT, no telemetry, no account. Plurals are translated per CLDR category. qwen3:8b got 8 of 8 Polish forms and 6 of 8 Russian in a live run, so for Slavic languages I'd use gemma4.

Happy to answer questions about the byte-stable writers or the prompt validation loop.
