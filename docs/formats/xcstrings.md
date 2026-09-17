# Xcode string catalogs (`.xcstrings`)

Xcode 15+ string catalogs: one JSON file per catalog holding the source language and every translation.

## Layout

`polygo init` finds every `*.xcstrings` under the project (skipping `.build`, `Pods`, `DerivedData`) and reads the target locales from the catalog's existing localizations.

```toml
[[files]]
format = "xcstrings"
path = "App/Localizable.xcstrings"     # no locale_path: the catalog holds every locale
```

Keys with `"shouldTranslate" : false` are skipped. Keys without a source `stringUnit` use the key itself as the source text (Xcode's default for `String(localized:)` keys).

## Plurals

Entries whose source language has `variations` (plural or device variations) are **preserved untouched and not translated** yet. `polygo check` still validates them: every locale must provide all CLDR plural categories the language needs (`pl` needs `one`, `few`, `many`, `other`; `ru`/`uk` likewise; `ja` only `other`), and reports `plural` errors otherwise — this is how the 44 missing Slavic forms in IceCubes were found.

## Placeholders

`%@`, `%lld`, `%d`, `%.2f`, `%1$@`, `%2$lld`, `%%` and the stringsdict-style `%#@name@`. Numbered arguments are compared as a set (reordering is fine, dropping or duplicating is not); unnumbered ones must appear in the same count and order. `% @` (a stray space) is flagged — the corpus has real shipped examples of that in DuckDuckGo's French and Spanish.

## What is preserved

The writer reproduces Xcode's own style so a diff shows only your strings: two-space indent, `"key" : value` with a space before the colon, Xcode's key order, empty objects as `{\n\n}`, raw UTF-8 (no `\uXXXX`), unescaped slashes, the file's line endings, and whether it ended with a newline. Verified byte-for-byte on 11 catalogs from Loop, Whisky, IceCubes, boring.notch, damus and six DuckDuckGo modules.

New translations are written as `"state" : "translated"` string units. `extractionState`, comments and `shouldTranslate` are never touched.
