# Known upstream placeholder bugs

`known_bugs.json` lists real source/translation pairs in the corpus that
`polygo check` flags and that we verified by hand are genuine bugs in the
upstream project's shipped translations (not checker false positives):

- DuckDuckGo macOS: `% @` (a space inside the placeholder) in es/fr/it/nl/pl —
  `Ouvrir dans % @`, `Wygasa: % @`, `Meer op % @` …; a dropped `%d` in pl
  (`… limit rozmiaru MB.`); `1$#@items@` and `%1$# @popups@` mangled variables;
  `% 5$d` in the Italian LastPass instructions.
- IceCubes: Polish `już istnieje` for `%@ already exists`; Catalan `% publicacions`;
  Korean `팔로워 %2$@명` where the source uses a `%#@followers@` variable; and the
  `notifications.label.*` keys whose English source (`starred`, `followed you` …)
  has no `%#@count@` variable while the de/eu/fr/nl translations do.
- boring.notch: pt-BR `Descrição` for `Custom notch size - %.0f`.

They stay in the corpus on purpose: the false-positive test asserts that
nothing *else* is flagged, and these show the check catches real-world breakage.
