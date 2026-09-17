# Flutter ARB

## Layout

`polygo init` reads `l10n.yaml` (`arb-dir`, `template-arb-file`) and derives the locale pattern from the template name (`app_en.arb` → `app_{locale}.arb`, `intl_en.arb` → `intl_{locale}.arb`).

```toml
[[files]]
format = "arb"
path = "lib/l10n/app_en.arb"
locale_path = "lib/l10n/app_{locale}.arb"
```

`@key` metadata (`description`, `placeholders`) is read from the template: the description becomes the developer comment the model sees, and the placeholder names are appended to it. Locale files carry only `@@locale` and the translated strings, in the template's key order: exactly what `flutter gen-l10n` expects.

## Plurals

ICU plural and select messages (`{count, plural, =0{...} one{...} other{...}}`) are translated inside the string. `polygo check` parses the ICU structure of source and translation and reports `placeholders` errors when an argument is renamed, a placeholder inside a branch is dropped, the `other` branch is missing, or braces are unbalanced. Translations may add branches the target language needs (`few`, `many`); whether they do is not yet checked against the CLDR table for ICU messages.

## Placeholders

`{name}`, `{count}`, `{date}` and nested ICU arguments. Names must match exactly (`{amount}` ≠ `{Amount}`).

## What is preserved

The span-based JSON parser keeps the template byte-for-byte; locale files that already exist keep their formatting, and only edited values are spliced. Verified on FluffyChat, Ente and Immich mobile bundles.
