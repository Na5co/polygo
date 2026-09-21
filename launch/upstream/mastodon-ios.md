# Issue for mastodon/mastodon-ios

Mastodon manages translations on Crowdin, so this goes in as an issue, not a PR.

**Title:** Albanian translations with missing or extra format placeholders

**Body:**

While checking string catalogs with a placeholder tool I noticed a few Albanian (`sq`) strings in `MastodonSDK/Sources/MastodonLocalization/Resources/Localizable.xcstrings` where the format specifier doesn't match the English source. The ones that will show wrong in the app:

- `Common.Controls.Actions.ShareUser`: `Share %@` → `Ndajeni:` (the name is gone)
- `Common.Controls.Status.UserReblogged`: `%@ boosted` → `U përforcua!` (the name is gone)
- `Common.Controls.Status.UserRepliedTo`, `Scene.Compose.ReplyingToUser`, `Scene.Search.Recommend.HashTag.PeopleTalking`, `Scene.Settings.Overview.Logout`: `%1$@` missing
- `Scene.Compose.Poll.OptionNumber`: `Option %ld` → `%ld nga %ld` (a second `%ld` with nothing to fill it)
- `Common.Controls.Keyboard.Timeline.ToggleFavorite` / `ToggleReblog`: contain `{name}`, which the source doesn't have
- `plural.people_talking` and `plural.filtered_notification_banner.subtitle`, `other` form: `%1$d` missing

I assume these need fixing on Crowdin rather than in the repo, so I'm reporting rather than opening a PR. Happy to help if there's a better route.
