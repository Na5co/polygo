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

## Deploy it to Cloud Run (free at this size)

Cloud Run's always-free tier is 2M requests, 180,000 vCPU-seconds and 360,000 GiB-seconds a
month — about **18,000 pull requests**, at the ~10 seconds a review takes. From a Google
Cloud project with billing enabled and `gcloud` installed:

```sh
PROJECT=your-project ./bot/deploy-cloud-run.sh ~/Downloads/your-app.private-key.pem
```

It enables the APIs, puts the App's key and webhook secret in Secret Manager, builds the
image with Cloud Build (your laptop is arm64, Cloud Run is amd64), deploys, and prints the
webhook URL and secret to paste into the App's settings. Run it again to ship a new build.

The flags that matter, and why:

- `--concurrency 1` — a review peaks around 150 MB; two at once would not fit in 512 MiB,
  so Cloud Run runs another instance instead of thrashing one.
- `--max-instances 3` — the ceiling on what a busy day can cost.
- `POLYGO_BOT_SYNC=1` — Cloud Run stops a container's CPU when the response goes out, so
  the review happens *before* the answer. A check that takes longer than GitHub's ten
  seconds then shows in the delivery log as timed out; the review is still posted, because
  the platform lets the handler finish after GitHub hangs up.

## Run it anywhere else

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
| `POLYGO_BOT_SYNC` | `1` to review before answering — for platforms that stop the CPU after a response (Cloud Run, and serverless generally). Leave unset on a normal server. |
| `POLYGO_BOT_MAX_REPO_MB` | skip repositories bigger than this (default 500) |
| `POLYGO_BOT_MAX_REVIEWS` | reviews one installation may ask for per 10 minutes (default 20) |

`GET /health` answers `polygo-bot ok`; the webhook is `POST /webhook` (or `/`). One
process, one container, no state: run it anywhere that can run a container, and scale it by
running more.

## Privacy and support

[bot/PRIVACY.md](PRIVACY.md) is the App's privacy policy — what it touches, what it keeps
(nothing), who else sees it (GitHub, and the platform it runs on). Support is
[the issue tracker](https://github.com/Na5co/polygo/issues).

## Its face

`docs/img/bot-avatar.svg` is the avatar: the wordmark's globe inside a speech bubble, with
an antenna and the wordmark's check. Shapes only — no fonts and nothing external — so any
renderer produces the same image. GitHub wants a PNG of at least 200×200 for an App, which
on a Mac needs no extra tool:

```sh
rsvg-convert -w 512 -h 512 docs/img/bot-avatar.svg -o avatar.png       # the App's logo
rsvg-convert -w 1280 -h 640 docs/img/feature-card.svg -o card.png     # the Marketplace card
```

(`brew install librsvg`. `qlmanage -t -s 512 -o . docs/img/bot-avatar.svg` works for the
square avatar without installing anything, but forces a square canvas, which crops the
card.)

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
when it finishes. Measured on the largest real project polygo has been pointed at
(open-webui: 3,484 keys × 61 locales, 140,961 translations) a review peaks at **146 MB** and
takes about two seconds of CPU, which is why 512 MiB is the size to run it at.

An App anyone can install is an App anyone can point anywhere, so the bot says no to two
things: a repository over `POLYGO_BOT_MAX_REPO_MB` (asked of the API before anything is
fetched) and an installation asking for more than `POLYGO_BOT_MAX_REVIEWS` in ten minutes.
A review holds at most 40 comments; the rest are counted in its summary. The App's API
calls are per installation, well inside GitHub's rate limits for anything short of a very
busy organization.
