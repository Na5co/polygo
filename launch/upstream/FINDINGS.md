# What `polygo check` finds in popular open-source apps

Scanned 2026-09-21 with `scripts/scan_upstream.sh` (raw output in `scans/`).
"Broken placeholders" are checker errors; the DuckDuckGo, Ice Cubes, Mastodon
and NewPipe ones below were verified by hand against the source string.
"Incomplete plural sets" means a language that needs `few`/`many` only has
`one`/`other`, so counts like 2, 3, 4 render the wrong form.

| App | Broken placeholders | Incomplete plural sets | Locales affected | Status |
|---|---:|---:|---:|---|
| duckduckgo/apple-browsers (macOS) | 12 | 0 | 5 | PR duckduckgo/apple-browsers#6852 |
| Dimillian/IceCubesApp | 3 | 44 | 3 + 3 | PR Dimillian/IceCubesApp#2503 |
| mastodon/mastodon-ios | 13 | 0 | 1 (sq) | issue draft below (Crowdin) |
| TeamNewPipe/NewPipe | 8 | 64 | 7 | issue draft below (Weblate) |
| tuskyapp/Tusky | 24 | 21 | 14 | not yet reported |
| signalapp/Signal-Android | 13 | 201 | 7 | not yet reported |
| element-hq/element-android | 133 | 175 | 28 | not yet reported |
| f-droid/fdroidclient | 2 | 1 | 2 | not yet reported |
| AntennaPod/AntennaPod | 0 | 69 | 0 | plurals only |
| calcom/cal.com | 219 | 0 | 22 | PR calcom/cal.diy#30213 (10 renames) |
| umami-software/umami | 40 | 0 | 20 | PR umami-software/umami#4556 (5 fixes) |
| AppFlowy-IO/AppFlowy | 70 | 0 | 18 | PR AppFlowy-IO/AppFlowy#9035 (14 renames) |
| open-webui/open-webui | 82 | 0 | 30 | PR open-webui/open-webui#30324 (25 fixes) |
| Lissy93/dashy | 26 | 0 | 12 | PR lissy93/dashy#2353 (1 fix) |
| formbricks/formbricks | 119 | 4 | 14 | branch pushed; repo blocks fork PRs via API |
| louislam/uptime-kuma | 181 | 0 | 40 | Weblate; not reported |
| TryGhost/Ghost | 16 | 0 | 5 | not reported |
| ToolJet/ToolJet | 57 | 0 | 12 | not reported |
| ether/etherpad-lite | 5 | 0 | 5 | translatewiki; not reported |
| homarr-labs/homarr | 36 | 0 | 8 | Crowdin; not reported |
| Infisical/infisical | 4 | 0 | 2 | not reported |
| LiveContainer/LiveContainer | 2 | 0 | 1 | Crowdin; not reported |

Totals across 23 apps: about 1,000 broken placeholders and 580 incomplete plural sets. Two polygo false positives were found and fixed on the way (Arabic dual forms may omit the count; an empty source value with a natural-language key).

Most Android projects take translations from Weblate, Crowdin or Transifex, not
from PRs to `strings.xml`. For those, file an issue (or fix on the platform);
PRs are for projects that edit the catalog in the repo (DuckDuckGo, Ice Cubes).

## Verified examples

**Mastodon iOS, Albanian**
- `Common.Controls.Actions.ShareUser`: `Share %@` → `Ndajeni:` (name dropped)
- `Common.Controls.Status.UserReblogged`: `%@ boosted` → `U përforcua!` (name dropped)
- `Scene.Compose.Poll.OptionNumber`: `Option %ld` → `%ld nga %ld` (a second `%ld` with no argument behind it)
- `Common.Controls.Keyboard.Timeline.ToggleFavorite`: `Toggle Favorite on Post` → contains a `{name}` the source never had

**NewPipe**
- `sign_in_confirm_not_bot_error`, bs/es/eu/ka/sr: the source has `%1$s` twice and `%2$s` once; these five translations dropped `%2$s`
- `did_you_mean`, ckb: `٪1$s` uses the Arabic percent sign (U+066A), so the placeholder is never substituted
- `export_subscriptions`, vi, `other` form: `%1$d` missing

**Signal Android**
- `ConversationItem_cant_download_*`, ky/pt: translations contain `{0}` (a Java MessageFormat placeholder) where the source uses none
- `IdealTransferDetailsFragment__enter_your_bank`, ar: `%1$s` missing

**Element Android**
- `{app_name}` missing from 40+ strings across ar, iw, fa, eu, bn-BD, bg
