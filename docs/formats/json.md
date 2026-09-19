# Translating i18next JSON locale files with polygo

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

i18next v4 suffixes (`key_one`, `key_other`, plus `key_zero`, `key_two`, `key_few`, `key_many`) are translated per CLDR category: an English `photos_one` / `photos_other` becomes `photos_one`, `photos_few`, `photos_many`, `photos_other` in `pl.json` and just `photos_other` in `ja.json`. Each form is requested with a concrete count and written into the group in CLDR order. In `polygo status` and `polygo.lock` the forms appear as `photos#plural.few`.

A group is a plural only when it has `_other` and at least one more form: `step_one` next to `step_two` (no `_other`) stays an ordinary key, and a lone `items_other` is i18next's opt-out (one string for every count) and is also left as is. `polygo check` reports a `plural` error when a locale file is missing a category its language needs.

## Placeholders

`{{name}}`, `{{count, number}}`, `$t(other.key)` nesting, `{name}` (vue-i18n/ICU style) and full ICU `{count, plural, one {...} other {...}}` messages, which are validated structurally and recursively.

## What is preserved

A position-tracking parser records the span of each string value; serialization replays the original text with only edited values spliced in, so indentation, key order, trailing newlines and unicode escapes stay as they were. New keys in a locale file are inserted following the source file's key order and the target file's indentation. Verified on Excalidraw, Hoppscotch, Jellyfin, Immich, Grafana and Formbricks.
