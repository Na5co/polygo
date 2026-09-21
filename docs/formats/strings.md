# Apple `.strings` (legacy iOS / macOS)

`en.lproj/Localizable.strings`, `de.lproj/Localizable.strings`, …: the format every Apple app used before String Catalogs, and most still ship. Files are `"key" = "value";` pairs with `/* comments */`, often UTF-16.

## Layout

`polygo init` finds `<dir>/<locale>.lproj/<Name>.strings` and writes one `[[files]]` per `Name` (`Localizable`, `InfoPlist`, …). The source is `en.lproj`, else `Base.lproj`. A `Name.xcstrings` catalog next to them wins (Xcode migrates `.strings` into it).

```toml
[[files]]
format = "strings"
path = "App/en.lproj/Localizable.strings"
locale_path = "App/{locale}.lproj/Localizable.strings"
```

## What is preserved

Only edited values are spliced; comments, spacing, key order, bare (unquoted) keys and `\U00e9` escapes stay as they were. New keys go where they are in the source file, with the source comment. A locale file that is UTF-16 (with a BOM, little- or big-endian) is read and written back as UTF-16; a new locale file takes the source file's encoding. Verified on Signal iOS's four 650 KB files (byte-stable, 0.08 s to check all 14,652 translations).

## Plurals: `.stringsdict`

`polygo check` reads every `.stringsdict` in the source `.lproj` (any name: Signal ships `PluralAware.stringsdict`) and its copy in each locale. For every `NSStringPluralRuleType` variable: the locale must have every CLDR category it needs (`plural` error, with the line), each form must keep the placeholders of the source's form (`placeholders` error), a variable the locale lacks is an error, and a key the locale's dict lacks is a warning (iOS falls back to the source language).

Two conventions are understood: inside a variable an unnumbered `%d` means that variable's argument (so `%d` in English and `%2$d` in German are the same), and in `zero`/`one`/`two` forms the number may be left out or added ("Ein Mitglied") — except in languages whose `one` also covers 21, 31, 101 (Russian, Ukrainian, Belarusian, Serbo-Croatian, Lithuanian, Latvian), where a `one` form without the number is reported: Signal's Russian says "более чем 1 элементом" for 21 items.

`translate` does not write `.stringsdict` yet.
