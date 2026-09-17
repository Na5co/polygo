# Translating .NET `.resx` and WinUI `.resw` with polygo

## Layout

`polygo init` recognises both conventions:

- side-by-side files: `Resources.resx`, `Resources.de.resx` → `locale_path = "Properties/Resources.{locale}.resx"`
- per-locale folders (UWP/WinUI): `Strings/en-US/Resources.resw`, `Strings/de-DE/Resources.resw` → `locale_path = "Strings/{locale}/Resources.resw"`

```toml
[[files]]
format = "resx"
path = "Strings/en-US/Resources.resw"
locale_path = "Strings/{locale}/Resources.resw"
```

Only text `<data>` elements are units; entries with `type=` or `mimetype=` (images, binary blobs, typed values) are skipped. `<comment>` text is shown to the model.

## Plurals

.NET resources have no plural syntax; strings are translated as-is. Anything using ICU (`{count, plural, ...}`) via a library is validated structurally by `polygo check`.

## Placeholders

`{0}`, `{1:N2}` (composite format; the format spec is ignored), `{name}` and `{{name}}`; every placeholder in the source must appear in the translation. `&amp;`, `&lt;` and `&apos;` are decoded for the model and re-encoded on write.

## What is preserved

Span-based: only edited `<value>` contents are replaced; the XML header comment, `<resheader>` block, `xml:space="preserve"`, attribute order and indentation are untouched. New entries are appended before `</root>` in the file's indentation style. New locale files copy the source's headers without any `<data>`. Verified on ShareX, NAPS2 and Files.
