# Android resources (`strings.xml`)

## Layout

`polygo init` finds every `res/values/strings.xml` (skipping `build/`) and reads the target locales from sibling `values-<qualifier>` directories.

```toml
[[files]]
format = "android"
path = "app/src/main/res/values/strings.xml"
locale_path = "app/src/main/res/values-{android_locale}/strings.xml"
```

`{android_locale}` turns `pt-BR` into `pt-rBR` and `zh-Hans` into `b+zh+Hans`; `{locale}` inserts the locale as written. Locale files that don't exist yet are created with the source file's XML declaration and indentation.

`translatable="false"` strings are skipped. Values are decoded (`\'`, `\n`, `&amp;`, `<![CDATA[…]]>`) for the model and re-encoded on write; Inline markup (`<b>`, `<xliff:g>`) is kept inside the value and passed to the model as text; the placeholders it wraps are still checked.

## Plurals

`<plurals>` are translated per quantity the target locale needs (`one`/`other` for German; `one`/`few`/`many`/`other` for Russian and Polish), each as its own unit (`imported#plural.few`). Missing `<item quantity="…">` elements are appended to an existing block in the file's indentation; a missing block is created before `</resources>`. Human-translated quantities are kept. `<string-array>` items are preserved but not translated.

`polygo check` validates every `<plurals>` in every locale against the CLDR category table (`plural` error when a locale lacks a required quantity).

## Placeholders

Java `String.format` families: `%s`, `%d`, `%1$s`, `%2$d`, `%.1f`, `%%`, plus `{name}` and `$name`-style tokens. Numbered arguments are compared as a set.

## What is preserved

The parser records the byte span of every translatable value and serialization replays the original file, splicing in only the edited values. Comments, attribute order, blank lines, CDATA, entity encoding and indentation survive untouched. Verified on the AntennaPod, NewPipe, DuckDuckGo, F-Droid and Thunderbird catalogs.
