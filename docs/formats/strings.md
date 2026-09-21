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

## Plurals

`.stringsdict` files are not read yet; plural rules for `.strings` projects live there. Placeholders (`%@`, `%1$@`, `%lld`) are checked like `.xcstrings`.
