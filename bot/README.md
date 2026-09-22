# polygo-bot — a GitHub App that reviews translations

A pull request touches `de.json`, and a minute later there is a review on it: an inline
comment on the line that broke, and — where the repair is mechanical, a translator having
translated the placeholder *name* — a suggested change the author commits with one click.

The review is [`polygo check --review`](../README.md#polygo-check). The bot adds no rules of
its own, so what it says on a pull request is exactly what `polygo check` says on a laptop.

**You may not need this.** The same review runs inside your own CI with the
[Action](../action/README.md) (`mode: check`, `review: "true"`) — no server to host, no app
to install, nothing leaving the runner. The App is for reviewing repositories you would
rather not add a workflow to, or for offering the review to other people's repositories.

## What it does with a pull request

1. Verifies the webhook signature; anything unsigned is refused.
2. Answers GitHub immediately (`202`) and does the work on its own thread.
3. Drops the event unless the diff touches a string file (`.xcstrings`, `strings.xml`,
   `.arb`, `.po`, `.resx`, `.strings`, a locale `.json`) — most pull requests never reach
   step 4.
4. Fetches `refs/pull/{n}/head` shallow (which exists on the base repository, so a pull
   request from a fork needs no access to the fork), reads `polygo.toml` or detects the
   layout, and runs the check.
5. Posts one review, commenting only on lines the diff touches, and skips anything it has
   already said on the same line — a second push does not repeat the review.
6. Deletes the working copy. Nothing is stored between events: no database, no queue, no
   token kept, no copy of anyone's strings.

It never blocks a pull request: the review is a `COMMENT`, and the repository's own CI
decides whether anything fails.

## Run it

```sh
docker build -f bot/Dockerfile -t polygo-bot .
docker run -p 8080:8080 \
  -e POLYGO_BOT_APP_ID=123456 \
  -e POLYGO_BOT_PRIVATE_KEY="$(cat app.private-key.pem)" \
  -e POLYGO_BOT_WEBHOOK_SECRET=… \
  polygo-bot
```

| variable | |
|---|---|
| `POLYGO_BOT_APP_ID` | the App's id |
| `POLYGO_BOT_PRIVATE_KEY` | the App's private key (PEM), or `POLYGO_BOT_PRIVATE_KEY_FILE` to read it from a file — a mounted secret |
| `POLYGO_BOT_WEBHOOK_SECRET` | the webhook secret you gave GitHub |
| `PORT` | default 8080 |

`GET /health` answers `polygo-bot ok`; the webhook is `POST /webhook` (or `/`). One
process, one container, no state: run it anywhere that can run a container, and scale it by
running more.

## Create the App

GitHub → Settings → Developer settings → **GitHub Apps** → New GitHub App.

- **Webhook URL**: where you deployed it, `https://…/webhook`. **Webhook secret**: any long
  random string, the same one in `POLYGO_BOT_WEBHOOK_SECRET`.
- **Repository permissions**: `Contents: Read-only` (to fetch the branch),
  `Pull requests: Read & write` (to leave the review), `Metadata: Read-only`.
- **Subscribe to events**: `Pull request`.
- Generate a private key; that `.pem` is `POLYGO_BOT_PRIVATE_KEY`.

Then install it on the repositories you want reviewed. The first pull request that touches a
string file gets a review.

## Cost and limits

Each event is one shallow fetch and one check: seconds of CPU and a few MB of disk, freed
when it finishes. A review holds at most 40 comments; the rest are counted in its summary.
The App's API calls are per installation, well inside GitHub's rate limits for anything
short of a very busy organization.
