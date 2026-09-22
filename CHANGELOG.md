# Changelog

All notable changes to polygo. Versions follow [SemVer](https://semver.org); dates are ISO.

## Unreleased

- `polygo check --review`: the findings as a GitHub pull-request review (JSON for `POST /pulls/{n}/reviews`) — one inline comment per file and line, and for a translated placeholder name a ```suggestion block the author commits with one click. The suggested line is produced by the format's own writer (applied to a copy of the tree and diffed), so committing it leaves a valid file. `--base <ref>` restricts the comments to the lines a branch changed; `--diff <file|->` takes the pull request's diff instead, for a reviewer with no checkout.
- `polygo-bot`: the same review as an installable GitHub App (`bot/`, a workspace member, not published to crates.io). One container, one endpoint, no database: it verifies the webhook signature, answers GitHub at once, skips pull requests whose diff touches no string file, fetches `refs/pull/{n}/head` shallow (so a fork needs no access), checks it, posts one review and deletes the working copy. Deploy with `bot/Dockerfile`; `bot/README.md` has the App's permissions and events. No new dependencies: the HMAC and the RSA signature come from `ring`, already in the tree through ureq.
- The Action's `mode: check` takes `review: "true"` (needs `pull-requests: write`): it reads the diff from the API, posts the review, and skips what it already said on the same line, so a second push does not repeat itself. No app, no server, nothing leaves the runner.

## 0.1.7 — 2026-09-22

- `placeholders`: a translated placeholder *name* (`{{ models }}` → `{{ modelli }}`, `%(count)d` → `%(anzahl)d`, `$name` → `$nombre`, a renamed ICU plural argument) is reported as such — *placeholder name translated: {{models}} → {{modelli}}* — instead of as one missing and one unexpected token. Names in any script count (`{{ модели }}`, `{{ماڈلز}}`), so the bug is seen where it lives. The finding carries the corrected translation (`"fix"` in `--json`, `properties.fix` in SARIF).
- `polygo check --fix` first applies those mechanical fixes with no model involved, and works without `polygo.toml`: `polygo check locales/ --fix`, `polygo check owner/repo --fix` (then `git diff` in the clone). What still needs a new translation goes to the configured model as before. On Open WebUI: 23 translations in 9 locales, a 24-line diff.
- Python `%(name)s` arguments are now real placeholders (they were read as prose): compared as a set, like numbered ones.
- `length`: no warning when the source is a key standing in for missing English (`dashboard.settings.none`: Penpot's msgids; 384 noise findings gone) or a 1–3 character abbreviation (`CVV` → `Cryptogramme visuel`, `AI` → `الذكاء الاصطناعي`).
- `polygo check <repo>`: a git URL, `git@…` remote or GitHub `owner/repo` instead of a path. Cloned shallow and single-branch into `~/.cache/polygo/repos/` (`POLYGO_CACHE_DIR`), refreshed on the next run, then checked like any directory; `--ref` picks a branch or tag. `polygo check Dimillian/IceCubesApp`: 19 s the first time, 1 s after.

## 0.1.6 — 2026-09-22

- `.po`: a `Plural-Forms` header whose `nplurals` does not match the language is a `plural` error (a Russian file saying `nplurals=2` is wrong before any string is; the message gives the rule to paste), and a file with plural entries but no header is a warning (gettext assumes two forms).
- `polygo check --explain <code>` (or `all`): what a code means and what to do, from the same table the SARIF rules use.
- `check --fix` re-translates only what a new translation can cure (placeholders, markup, empty, glossary, length) in target locales; it no longer tries to "fix" a duplicate key, a short array, or an Android escape in the source file (which asked the engine to translate into the source locale).
- `check --locale xx` with a locale that is not a target is an error, as in `translate`, instead of a silent "nothing to check".
- In projects with several `[[files]]`, file-level findings (Android `escape`/`array`, `.stringsdict`, `orphan`, `duplicate`, `state`) now carry the `path:key` prefix like every other finding, so `polygo:ignore` and the baseline resolve to the right file. A baseline written by 0.1.5 for such a project needs `--write-baseline` again.
- Internals: every finding goes through one emitter that applies skip rules, prefixes the key and finds the line; codes and severities are an enum that owns the titles and explanations. 10,156 corpus findings verified byte-identical before and after.

## 0.1.5 — 2026-09-22

- `polygo check --sarif` prints SARIF 2.1.0 with a rule per code and stable fingerprints; the Action's `sarif: "true"` uploads it to GitHub code scanning (Security tab, new/fixed history on PRs). The baseline applies to it like every other output.
- `inconsistent` (warning): the same short source term translated two ways in one locale (`Settings` → `Réglages` in 3 keys, `Paramètres` in 1); the minority gets the warning with the counts. Untranslated copies do not vote, case and inflection (`aucun`/`aucune`) are the same word, a tie is a choice. Ice Cubes: 45 across 12,761 translations.
- Per-code ignores: `polygo:ignore=identical,length` in a developer comment, or `[keys] ignore = { "legal.*" = ["length"] }` in `polygo.toml`, keep `check` quiet about those codes for those keys without hiding the key from everything the way `skip` does.
- `.stringsdict` is checked: every `NSStringPluralRuleType` variable needs the locale's CLDR categories, each form keeps the source form's placeholders (an unnumbered `%d` inside a variable is that variable's argument, so `%d` vs `%2$d` agree; nested `%2$#@total@` references resolve), a missing variable is an error, a key the locale lacks is a warning. Any file name in the source `.lproj` (Signal: `PluralAware.stringsdict`).
- Plural forms are compared with the language in mind, everywhere (`.xcstrings`, Android, `.po`, `.stringsdict`, and the repair loop in `translate`): in `zero`/`one`/`two` the number may be left out or added in languages where those are exact counts, but not where `one` also covers 21, 31, 101 (ru, uk, be, hr, sr, bs, lt, lv). Before, every `zero`/`one`/`two` form skipped the placeholder check entirely, which hid Signal's Russian "более чем 1 элементом" (shown for 21 items) and would have let `%@` mismatches through in those forms. Xcode's `%arg` is recognised as the substituted count.
- `polygo check --write-baseline` records every current finding by (file, key, locale, code) in `polygo-baseline.json`; from then on `check` hides and does not count known findings, fails only on new ones, and says how many baseline entries no longer match so the file can be pruned. `--no-baseline` reports everything; `--json` carries `baseline: {known, stale}`. The GitHub Action and `--github` honour it automatically.
- Seven more bug classes in `check`. Errors: `duplicate` (the same key twice in one file; the last one wins silently), `array` (an Android `<string-array>` with a different item count than the source: `IndexOutOfBounds` at runtime), unnumbered Android format arguments (`%s of %s` is an `aapt` error unless `formatted="false"`), `glossary` (`glossary.toml` is now enforced by `check`, not only by `translate`). Warnings: `encoding` (mojibake like `Ã©`, `â€™`: a file saved in the wrong encoding), `invisible` (zero-width space, mid-string BOM, U+2028/9, bidi embedding controls, C0 controls; ZWNJ/ZWJ/LRM/RLM are left alone), `link` (a URL or email address in the source that the translation changed or dropped), `brackets` (a pair the source keeps balanced and the translation does not; guillemets excluded, German reverses them), `entities` (`&amp;amp;`). Across 11 String Catalogs, 5 Android apps, Signal iOS and two gettext projects: three genuine bracket findings, nothing else.

## 0.1.4 — 2026-09-21

- Apple `.strings` (legacy iOS/macOS, `en.lproj/Localizable.strings`) is a supported format: `init` detects `*.lproj` layouts (one spec per file name, `en` or `Base` as source, a same-name `.xcstrings` wins), `check` runs every validator with lines, `translate` writes new keys in source order with the source comment. UTF-16 files (with BOM) are read and written back as UTF-16. Signal iOS: 14,652 translations checked in 0.08 s; two French strings carry `<strong>` tags the English does not. `.stringsdict` is not read yet.
- `init` writes `target_locales` on one line, like `add` does.
- `check` text output shows the first 20 findings of each code and then `… 3368 more state`, so a catalog with thousands of `needs_review` strings stays readable; `--all` lists every one, `--json` always has them all.
- New `check` warnings: `orphan` (a key the locale file has and the source does not, in every per-locale format; i18next plural forms the locale needs are not orphans), `fuzzy` (gettext `#, fuzzy`: shipped as untranslated at runtime), `state` (`.xcstrings` units marked `needs_review`/`stale`/`new` in Xcode, and keys whose `extractionState` is stale). Penpot's German `.po` has exactly its 4 fuzzy entries flagged; boringnotch ships 3,374 strings Xcode says need review.
- `check` ends with a coverage line when a locale is not fully translated (`coverage: pl 83% (1 of 6 missing)`; worst six locales, then a count), and `--json` gains `coverage: {locale: [translated, total]}`. The job summary from `--github` shows it too.

## 0.1.3 — 2026-09-21

- `check` findings carry the file a fix goes in and the line of the key: `locales/de.json:12` in the text output, `line=` in `--github` annotations (they now land on the exact line of the PR), `"line"` in `--json`. For `.xcstrings` the line is the locale's entry inside the key. Per-locale formats now name the locale file, not the source file.
- New checks: `markup` (error: a `<b>`, `</a>` or `<br>` the source has and the translation lacks, or the reverse), `whitespace` (warning: leading/trailing space or newline dropped or added), `punctuation` (warning: the source ends with `:` `.` `!` `?` `…` and the translation ends with a letter; any script's marks pass), and for Android `escape` (error: unescaped `'`, or a leading `@`/`?` that is not a resource reference, both of which fail `aapt`; warning: a stray unbalanced `"`, which Android silently drops from the text). On the DuckDuckGo macOS catalog these add 26 real warnings; on 3,756 strings from five Android apps, 0 false positives and 3 shipped stray quotes.
- `check` on a large catalog no longer scans the file per finding (IceCubes: 189 s → 3.7 s in a debug build).
- Translation memory no longer learns a hand-edited translation whose placeholders do not match the source, and never answers for keys that `check --fix` / `audit --fix` are re-translating (it could hold exactly the broken text). Found by isolating the test suite from the developer's own memory file.
- `polygo.toml` typos are pointed out: `batch_szie = 5` prints `unknown key \`batch_szie\` (did you mean \`batch_size\`?)` instead of silently doing nothing, and a misspelled required key (`target_locale`) is named in the parse error. Warnings, not errors, so an older polygo still reads a newer file.
- First `polygo translate` with an Ollama model that is not pulled offers to pull it right there (`[Y/n]`); `--yes` for scripts. Without a terminal it says the two ways to pull.
- GitHub Action `mode: check`: validates the repository's translations on every pull request with no model and no secrets, one annotation per finding on the file, a table in the job summary, job fails on errors (`strict: "true"` for warnings too), `path:` for repos without `polygo.toml`. Outputs `errors` / `warnings`.
- `polygo check --github` prints GitHub workflow-command annotations and writes the job summary when `GITHUB_STEP_SUMMARY` is set, for any CI on GitHub.
- The `v0` tag the Action docs pointed at (`Na5co/polygo/action@v0`) did not exist; it does now and moves with every 0.x release.
- Pre-commit snippet in `action/README.md`.
- `polygo check <file-or-directory>` works without `polygo.toml`: the format and locales are detected (a single `res/values/strings.xml` or `locales/en.json` is placed by looking at its parent directories), every validator runs, and the note printed says what was checked. Plain `polygo check` in a project with no config does the same for the current directory. `--fix` still needs a configured project. The ten-second "does my existing localization have bugs?" path, no model involved.
- `init` (and `check`) inside the layout directory itself (`cd res && polygo init`) no longer writes absolute `locale_path` templates.

## 0.1.2 — 2026-09-20

- Release pipeline is back: `scripts/release.sh <version>` bumps, checks, commits and tags; pushing the tag builds five binaries, `SHA256SUMS`, the Homebrew formula (`scripts/formula.sh`, which now installs shell completions) and the GitHub release, and publishes to crates.io and the tap when the secrets exist. CI runs fmt/clippy/tests on every PR. See `docs/dev/RELEASING.md`.
- Failures that retrying cannot fix stop the run at once with the problem and the fix on two lines, the way `doctor` reports: Ollama not running (`ollama serve`), model not pulled (`polygo use <model>`), no or rejected API key (which env var, or `polygo use … --api-key`), unknown model at an API endpoint. Timeouts, 429 and 5xx are still retried three times. Previously every one of these went through three backoff rounds and ended in a chain of socket errors.
- `check` says what it looked at: `check: ok (42 translation(s) in 3 locale(s))`, or `nothing to check yet … polygo translate first` on a fresh project.
- `polygo status --keys` (`-k`) lists the keys behind each count, with the quarantine reason for `needs-review` ones; `--json` gains a `keys` map per locale. `status --locale de` narrows it.
- `[keys] skip = ["debug.*", "internal_*"]` in `polygo.toml`: globs over the key that translate, status and check leave alone, for i18next JSON (no comment field for `polygo:skip`) and for whole families of keys in any format. `status` reports how many keys were skipped.
- Android `<string-array>` items are translated, one unit per item (`sort_modes#array.0`), with the whole list in the prompt so the items stay parallel. A missing array in a locale file is created from the source and filled item by item; a short one is padded before the new item lands, so an array is never left shorter than the original. `translatable="false"` arrays are skipped as before.
- `polygo completions <shell>` prints a completion script for bash, zsh, fish, elvish or PowerShell.
- i18next JSON plurals are translated per CLDR category: `photos_one` / `photos_other` in `en.json` produces `photos_one`, `photos_few`, `photos_many`, `photos_other` for Polish and only `photos_other` for Japanese, written in CLDR order next to the group. Previously only the source's own suffixes were written and `polygo check` then failed on polygo's own output for Slavic, Arabic and other multi-form locales.
- `check` no longer treats `step_one` + `step_two` (no `_other`) or a lone `items_other` as a plural group.
- `polygo add`, `remove` and `use` edit `polygo.toml` in place: comments and formatting survive, `target_locales` stays on one line.
- `init` in a project with no target locales, and `translate` with an empty `target_locales`, now say to run `polygo add <locale>` instead of reporting everything up to date.

## 0.1.1

- `polygo extract`: pull UI text out of JSX, HTML in template literals and .html/.vue/.svelte into `locales/en.json`. `--rewrite` replaces the strings in .ts/.tsx/.js/.jsx with `t("key")` calls and generates `src/i18n.ts`. Sentences with inline markup are extracted whole. `[extract] ignore` / `ignore_paths`, `--ignore`, `--ignore-path`.
- `polygo add` / `polygo remove` edit target locales.
- Batches are capped by source text length and Ollama gets a 16k context, so paragraphs no longer overflow the model.
- `translate` prints the plan and per-batch progress.
- Placeholder mismatches are caught in the repair loop, not only by `check`.
- Fragment check no longer flags HTML entities, handles, paths or hyphenated tokens.
- `init` explains how to start when an app has no string files yet.

## 0.1.0

First release.

- Formats: Xcode `.xcstrings`, Android `strings.xml`, i18next JSON, Flutter ARB, gettext `.po`, .NET `.resx`/`.resw`. Every writer is byte-stable (35 real-world files in `tests/corpus` round-trip exactly).
- `polygo init` detects the project layout and locales for all six formats.
- `polygo translate`: lockfile-driven incremental translation, batches, parallel jobs, crash-safe resume, `--dry-run`, `--retry-review`, `--no-context`. Human-edited translations are never overwritten.
- Context retrieval: code-usage snippets (Aho-Corasick over the source tree) and similar already-translated strings, within a token budget.
- Validation and repair of every model answer: placeholders, glossary, key echo, runaway length, identical-to-source; one repair round, then quarantine as `needs-review`.
- Plural translation: `.xcstrings` `variations.plural` (top-level and `%#@var@` substitutions), Android `<plurals>`, gettext `msgid_plural`. One unit per CLDR category the target locale needs, written into the native structure.
- `polygo check`: printf / ICU / i18next / composite-format placeholders, CLDR plural categories, empty/identical/length, untranslated fragments in non-Latin-script targets; `--json`, `--strict`, `--fix`.
- `polygo doctor`: checks config, files, provider reachability and that the model is pulled, and prints the fix for each failure.
- `polygo audit`: a second model grades translations 1 to 5 with reasons; `--fix` re-translates flagged strings, never human edits.
- Translation memory across projects (`~/.config/polygo/memory.toml`): human entries reused verbatim, model entries as few-shot examples; `polygo memory`.
- `polygo pseudo`: pseudo-locale (`en-XA`) with placeholders preserved.
- `polygo status --markdown`: coverage table with flags; the GitHub Action puts it in the PR body.
- Per-key directives in developer comments: `polygo:skip`, `polygo:max=N`.
- `polygo models` / `polygo use <model>`: model catalog; pulls Ollama models with progress; `openai/...`, `anthropic/...` or `--base-url` for API providers; `--api-key` stored with mode 0600; `--global` default for `init`.
- `polygo status`, `polygo review` (localhost-only approval page), GitHub Action (`Na5co/polygo/action`).
- Providers: Ollama (default, `qwen3:8b`), any OpenAI-compatible endpoint, Anthropic, deterministic mock.
- Distribution: prebuilt binaries for macOS (arm64, x86_64), Linux (musl, arm64, x86_64), Windows; `install.sh`, Homebrew tap, `cargo binstall`.

### Known limitations

- `.xcstrings` device variations are preserved but not translated. (Android `<string-array>` items: see Unreleased above.)
- ICU plural messages inside JSON/ARB strings are translated as a whole; `check` validates their structure but not whether the translation added the categories the locale needs.
- An 8B local model gets some Slavic inflections wrong (live test: 8/8 Polish, 6/8 Russian plural forms); use a larger model for those.
