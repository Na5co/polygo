# gettext `.po`

Django, Rails (fast_gettext), Python, PHP, GNOME apps, Penpot, Odoo, anything with `msgid`/`msgstr`.

## Layout

`polygo init` recognises both common trees:

- `locale/<locale>/LC_MESSAGES/<domain>.po` → `locale_path = "locale/{locale}/LC_MESSAGES/django.po"`
- flat `translations/<locale>.po` → `locale_path = "translations/{locale}.po"`

```toml
[[files]]
format = "po"
path = "locale/en/LC_MESSAGES/django.po"
locale_path = "locale/{locale}/LC_MESSAGES/django.po"
```

Entries are keyed by `msgctxt` + `msgid` (the gettext convention), so identical `msgid`s in different contexts are translated separately. `#.` extracted comments and `#:` source references are shown to the model as context. Multi-line `msgid "" "..."` strings are joined for translation and re-wrapped on write.

New locale files are created with a header copied from the source, `Language:` set and `Plural-Forms:` filled in from the CLDR table (`nplurals=3` for Russian, Polish...).

## Plurals

`msgid_plural` entries are translated into every `msgstr[n]` slot the locale file's `Plural-Forms:` header declares. Slots are labelled by their CLDR role (Russian `nplurals=3` → `one`, `few`, `many`; Arabic `nplurals=6` → `zero` ... `other`; Slovenian, Latvian, Romanian and the two-form default are also known), and the model is asked for each form with a concrete count. Slots that already hold a translation are kept. New locale files get the right `Plural-Forms` line and a `msgid_plural` block with empty slots before the forms are filled. A locale whose `nplurals` polygo cannot map is left untouched (`polygo doctor` and `translate -v` will show no plural units for it).

## Placeholders

Python `%(name)s`, `%s`, `%d`, `{name}` / `{0}` (str.format), `%1$s` (PHP), and `{{name}}`. Named arguments are compared as a set.

## What is preserved

Span-based: the original text is kept and only edited `msgstr` regions are replaced, so headers, translator comments, `#, fuzzy` flags, obsolete `#~` entries and wrapping style stay as they were. Verified on Django's and Penpot's English and German catalogs (300+ entries each).
