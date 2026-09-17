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

Plural `variations`: both top-level and inside `substitutions` (`%#@count@`): are translated one CLDR category at a time: German gets `one`/`other`, Polish `one`/`few`/`many`/`other`, Japanese only `other`. Each form is a separate unit (`key#plural.few`) in the lockfile and in `polygo status`, so a human-edited `few` is never overwritten. The model is told the concrete count each category stands for ("the form used when the count is 3"), which is what makes small models separate `few` from `many`. Substitution metadata (`argNumber`, `formatSpecifier`) is mirrored from the source; the outer string (`%#@count@ selected`) is translated as an ordinary unit.

An explicit `zero` in the source is mirrored into every locale. Device variations are preserved untouched and not translated.

`polygo check` validates every locale's plural against the CLDR table (`pl` needs `one`, `few`, `many`, `other`; `ja` only `other`) and reports `plural` errors otherwise: this is how the 44 missing Slavic forms in IceCubes were found.

## Placeholders

`%@`, `%lld`, `%d`, `%.2f`, `%1$@`, `%2$lld`, `%%` and the stringsdict-style `%#@name@`. Numbered arguments are compared as a set (reordering is fine, dropping or duplicating is not); unnumbered ones must appear in the same count and order. `% @` (a stray space) is flagged: the corpus has real shipped examples of that in DuckDuckGo's French and Spanish.

## What is preserved

The writer reproduces Xcode's own style so a diff shows only your strings: two-space indent, `"key" : value` with a space before the colon, Xcode's key order, empty objects as `{\n\n}`, raw UTF-8 (no `\uXXXX`), unescaped slashes, the file's line endings, and whether it ended with a newline. Verified byte-for-byte on 11 catalogs from Loop, Whisky, IceCubes, boring.notch, damus and six DuckDuckGo modules.

New translations are written as `"state" : "translated"` string units. `extractionState`, comments and `shouldTranslate` are never touched.
