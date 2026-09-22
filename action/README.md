# polygo GitHub Action

Two modes. `check` needs no model, no secrets and no `polygo.toml`; `translate` needs a
provider and opens a pull request.

## `mode: check` — fail the build on broken translations

```yaml
name: i18n
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Na5co/polygo/action@v0
        with:
          mode: check
          # path: app/src/main/res     # a file or directory when there is no polygo.toml
          # strict: "true"             # warnings (length, identical) fail too
          # sarif: "true"              # also upload to code scanning (needs security-events: write)
```

Every finding is a GitHub annotation on the file in the pull request, the job summary gets a
table, and the job fails on any error. With `sarif: "true"` (and `permissions:
security-events: write`) the findings also go to **code scanning**: the Security tab, with
new / fixed / still-open history across commits, like CodeQL. Every finding is one of: a placeholder that went missing in a translation, a
plural form Polish or Arabic needs and doesn't have, a string left half in English.
Outputs: `errors`, `warnings`.

Any CI, not just this Action: `polygo check --github` prints the same annotations, and
`polygo check --json` / `--strict` work everywhere.

Existing project with hundreds of warnings? Run `polygo check --write-baseline` once and
commit `polygo-baseline.json`: the Action then fails only on findings that are new.

### `review: "true"` — a reviewer on the pull request

```yaml
    permissions:
      contents: read
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
      - uses: Na5co/polygo/action@v0
        with:
          mode: check
          review: "true"
```

polygo leaves an **inline comment** on each line of the diff it has something to say about,
and where the repair is mechanical — a translator translated the placeholder *name*,
`{{ modelli }}` for `{{ models }}` — a **suggested change** the author commits from the
review with one click. The suggestion is the whole line as polygo's own writer would write
it, escaping and indentation included.

It only ever comments on lines this pull request touches (the diff comes from the API, so a
shallow checkout is fine), it posts `COMMENT` rather than blocking the PR — the job's exit
code is what fails the build — and it skips anything it has already said on the same line,
so a second push does not repeat the review. Findings elsewhere in the files are counted in
the review's summary and listed in the job summary.

No server and no app to install: it runs in your own CI with the repository's token, and
nothing leaves the runner.

## `mode: translate` — translate on push and open a PR

```yaml
name: translations
on:
  push:
    branches: [main]
jobs:
  translate:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
      - uses: Na5co/polygo/action@v0
        env:
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}   # or ANTHROPIC_API_KEY, or point polygo.toml at your own endpoint
        with:
          mode: translate          # the default
          locales: de,fr,ja        # optional
          pr: "true"               # "false" commits straight to the branch
```

The action never overwrites human edits (they are recorded as `edited` in `polygo.lock`) and
leaves anything the model could not translate well as `needs-review` for `polygo review`.
Outputs: `changed`, `coverage` (a Markdown table for the PR body).

## Pre-commit

`.pre-commit-config.yaml`, with polygo on PATH:

```yaml
repos:
  - repo: local
    hooks:
      - id: polygo-check
        name: polygo check
        entry: polygo check
        language: system
        pass_filenames: false
        files: \.(xcstrings|xml|json|arb|po|resx|resw)$
```

Or a plain git hook: `printf '#!/bin/sh\nexec polygo check\n' > .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit`.

## Versions

`@v0` follows the latest 0.x release; pin `@v0.1.3` for an exact one. The action installs the
matching polygo binary (`version:` overrides it).
