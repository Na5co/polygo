# polygo-bot — privacy

*Last updated: 23 September 2026. The app is [polygo-bot](https://github.com/apps/polygo-bot),
published by [@Na5co](https://github.com/Na5co) and built on
[polygo](https://github.com/Na5co/polygo), which is MIT-licensed and readable in full —
including [the bot itself](https://github.com/Na5co/polygo/tree/main/bot).*

## What it stores

Nothing.

There is no database, no queue and no object storage. The service keeps no copy of your
code, your strings, your translations or your findings, and it remembers nothing between
one pull request and the next.

## What it does with your repository

When a pull request touches a localization file, and only then:

1. It asks GitHub for the pull request's diff, and stops there unless a string file is in
   it.
2. It fetches `refs/pull/<n>/head` **shallow** (one commit) into a temporary directory
   inside the container.
3. It parses the localization files, compares translations against the source, and builds
   the review.
4. It posts the review through the GitHub API.
5. It deletes the temporary directory. When the container itself stops, everything in it
   goes with it.

No part of your repository is sent anywhere else. In particular: **no language model is
involved**, and nothing is sent to any model provider or third-party service. The check is
deterministic code running in the container.

## What it holds while it works

- **An installation access token**, obtained from GitHub per event and held in memory for
  the seconds the review takes. It is never written to disk and never logged.
- **The contents of your localization files**, in memory, for the same seconds.

## What it logs

One line per event, of the form `owner/repo#12: 3 comment(s) posted`, plus errors. Logs
hold repository names, pull-request numbers and counts — never file contents, never tokens.
They are kept by the hosting platform (Google Cloud Run, region `us-central1`) under its
default retention and are used to see that the service is working.

## Permissions, and why each one

| permission | why |
|---|---|
| Contents: **read** | to fetch the branch and read its localization files |
| Pull requests: **read & write** | to read the diff and leave the review |
| Metadata: read | required by GitHub for any app |

It never writes to your repository: it opens no branches, pushes no commits and changes no
files. A suggestion it leaves is applied by *you* clicking **Apply suggestion**, as a commit
authored by you. It posts reviews as `COMMENT` and never approves or blocks a pull request.

## Sub-processors

- **GitHub** — the source of every event and the destination of every review.
- **Google Cloud Run** (Google LLC) — where the container runs.

No analytics, no trackers, no advertising, no third-party telemetry.

## Removing it

Uninstall the app from **Settings → Applications → Installed GitHub Apps**. Its access ends
immediately. Since nothing is stored, there is nothing to delete and nothing to request.

## Security problems

Report anything you find privately through
[GitHub's security advisories](https://github.com/Na5co/polygo/security/advisories/new) for
the polygo repository, or by email (see Support below). Please do not open a public issue
for a vulnerability.

## Support

Open an issue at [github.com/Na5co/polygo/issues](https://github.com/Na5co/polygo/issues).
