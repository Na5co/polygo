# Show HN

## Title (80 chars max)

Show HN: Polygo, a local-first CLI that translates your app's strings with Ollama

## URL

https://github.com/Na5co/polygo

## First comment (post right after submitting)

Author here. This started as "I don't want to pay Lokalise $149 a month for a small app", but the part that turned out to matter is the checker.

polygo check reads your string files and flags translations that will break at runtime: a placeholder that went missing or got translated ({{ modelli }} for {{ models }}), a plural form Polish needs that isn't there, unbalanced braces. No model, no config, milliseconds.

I ran it over 23 popular open source apps last week and found about a thousand of these in production. Nobody had noticed because they only show up in a language the maintainers don't read. Fixes are merged in Open WebUI and Dashy so far, with more open in AppFlowy, Cal.diy, Umami, DuckDuckGo's macOS browser and Ice Cubes. All of them are one-line placeholder swaps, verified against the source string.

The translate side: polygo init finds your .xcstrings, Android, Flutter, i18next, gettext or .resx files, polygo translate fills in what's missing with a local model through Ollama (or any OpenAI/Anthropic endpoint), and writes back into the same files so git diff is just the new strings. Before translating "Open" it greps your code for Button("Open") so the model knows it's a menu item. A lockfile means hand edits are never overwritten.

One 4 MB Rust binary, MIT, no account, no telemetry. Happy to answer anything.
