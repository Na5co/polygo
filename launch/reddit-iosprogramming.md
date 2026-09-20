# r/iOSProgramming

**Title:** I built a CLI that translates .xcstrings catalogs with a local model and checks every %@ and plural. Free, MIT.

**Body:**

I didn't want to pay $149/month for Lokalise for a small app, and pasting the catalog into ChatGPT and merging it back by hand got old fast. So I wrote a small Rust CLI.

    polygo init
    polygo translate
    polygo check

It writes back in Xcode's exact format, so git diff shows only the new strings. A lockfile remembers what it wrote, so your hand edits are never overwritten.

A few things I'm happy with:

- It reads your Swift code first. "Open" inside Button("Open") gets translated as a menu item, not a verb.
- Plurals are done per language: Polish gets one/few/many/other, Japanese gets one form.
- check catches broken placeholders and missing plural forms. Run on DuckDuckGo's shipped catalog it found 12 errors, including "Ouvrir dans % @".

Runs on Ollama, nothing leaves your machine. Any OpenAI or Anthropic key works too.

https://github.com/Na5co/polygo

Question for people shipping more languages than me: do you keep translations in the catalog, or export xliff and round-trip?
