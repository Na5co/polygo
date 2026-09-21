# Issue for duckduckgo/apple-browsers

Before filing: check the strings against the current `macOS/DuckDuckGo/Localization/Localizable.xcstrings` on main.
These were found on a copy fetched 2026-09-17; anything fixed since should be dropped from the list.

**Title:** Broken format placeholders in macOS Localizable.xcstrings (fr, it, es, nl, pl)

**Body:**

Hi, I ran a placeholder checker over `macOS/DuckDuckGo/Localization/Localizable.xcstrings` and found a handful of translations where the format specifier got mangled, so the value is never substituted and users see a literal `% @` or nothing. Sharing in case it's useful; happy to open a PR.

Space inside the specifier (`% @` instead of `%@`):

- `open.in`, fr: `Ouvrir dans % @`
- `open.in`, it: `Apri in % @`
- `autofill.popover.autosave.text`, es: `Contraseña guardada para % @`
- `pm.card.expires.format`, pl: `Wygasa: % @`
- `preferences.about.more-at`, nl: `Meer op % @`
- `tooltip.clearHistory`, it: `Cancella la cronologia di navigazione per % @`
- `tooltip.clearHistoryAndData`, it: `Cancella la cronologia di navigazione e i dati per % @`

Placeholder dropped or altered:

- `aichat.attachment.file.exceeds.conversation.limit`, pl: `Załączone pliki przekraczają całkowity limit rozmiaru MB.` (the `%d` is gone, so the limit reads "MB" with no number)
- `import.credit-cards.from.source.automatic.error`, fr: ends in a bare `%`
- `fire.dialog.history.count`, pl: `Usuń 1$#@items@.` (should be `%#@items@`)
- `permission.popup.title`, it: `Bloccato %1$# @popups@` (should be `%#@popups@`)
- `import.csv.instructions.lastpass.new`, it: contains `% 5$d` (should be `%5$d`)

Found with [polygo](https://github.com/Na5co/polygo) (`polygo check`), a small open-source CLI I'm working on.
