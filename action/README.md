# polygo GitHub Action

Translates new or changed strings on every push and opens a pull request.

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
      - uses: atanasa/polygo/action@v0
        env:
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}   # or ANTHROPIC_API_KEY, or point polygo.toml at your own endpoint
        with:
          locales: de,fr,ja        # optional
          pr: "true"               # "false" commits straight to the branch
```

The action never overwrites human edits (they are recorded as `edited` in `polygo.lock`) and
leaves anything the model could not translate well as `needs-review` for `polygo review`.
