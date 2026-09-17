# i18next / JSON locale files

Flat or nested objects of strings, as used by i18next, react-i18next, vue-i18n, Next.js apps and most web projects.

## Layout

`polygo init` recognises three layouts:

- `locales/en.json`, `locales/de.json` → `locale_path = "locales/{locale}.json"`
- `locales/en/common.json`, `locales/en/errors.json` (namespaces) → one `[[files]]` per namespace with `locale_path = "locales/{locale}/common.json"`
- a single `locales/en.json` with no siblings → detected, target locales left for you to fill in

```toml
[[files]]
format = "json"
path = "public/locales/en/translation.json"
locale_path = "public/locales/{locale}/translation.json"
```

Nested objects become dotted keys (`settings.title`); arrays of strings are indexed (`tips.0`). Non-string values are left alone.

## Plurals

i18next v4 suffixes: `key_one`, `key_other`, `key_zero`, `key_few`, `key_many`: are translated as ordinary strings, and `polygo check` verifies that each plural group in a locale has every category that language needs (`plural` error when `pl` has only `_one`/`_other`, for instance). Locales that only need `other` are not asked for more.

## Placeholders

`{{name}}`, `{{count, number}}`, `$t(other.key)` nesting, `{name}` (vue-i18n/ICU style) and full ICU `{count, plural, one {...} other {...}}` messages, which are validated structurally and recursively.

## What is preserved

A position-tracking parser records the span of each string value; serialization replays the original text with only edited values spliced in, so indentation, key order, trailing newlines and unicode escapes stay as they were. New keys in a locale file are inserted following the source file's key order and the target file's indentation. Verified on Excalidraw, Hoppscotch, Jellyfin, Immich, Grafana and Formbricks.
