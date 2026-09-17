# Show HN

## Title (≤ 80 chars)

Show HN: Polygo – Lokalise for one person, a local-first CLI that translates your app

## URL

https://github.com/Na5co/polygo

## First comment (post right after submitting)

Hi HN, author here.

I ship a small Mac app in five languages. Lokalise starts at $149/month and wants my strings on its servers; the free alternative is pasting a string catalog into a chat window and hand-merging the answer. Neither felt right for a one-person project, so I built the thing I wanted:

- one static Rust binary, `polygo init && polygo translate`
- reads `.xcstrings`, Android `strings.xml`, i18next JSON, Flutter ARB, gettext `.po`, `.resx`
- writes back into your existing files byte-for-byte (the diff shows only the strings that changed)
- default model is `qwen3:8b` on Ollama, so it runs with no API key and no network; any OpenAI-compatible or Anthropic endpoint works too
- a lockfile tracks which source strings changed, so it re-translates only those and never overwrites a human edit

The part I'm most pleased with: before translating "Open" it greps your code for `Button("Open")` and tells the model it is a menu item, and it attaches the three most similar strings you already translated. In a blind A/B judged by a different model, that context won 9:5 with the rest tied.

It also checks placeholders and CLDR plural categories. Run against real repos it found 32 shipped placeholder bugs in DuckDuckGo's macOS browser and 44 missing Polish/Ukrainian/Belarusian plural forms in Ice Cubes — the corpus and the findings are in the repo.

MIT, no telemetry, no account. Plurals are translated per CLDR category with a concrete count in the prompt; in a live run qwen3:8b got 8/8 Polish forms and 6/8 Russian, so for Slavic languages I'd point it at a bigger model — the workflow is the same.

Happy to answer anything about the byte-stable writers or the prompt validation loop.
