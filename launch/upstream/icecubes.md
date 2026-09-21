# Issue for Dimillian/IceCubesApp

Before filing: check the strings against the current `IceCubesApp/Resources/Localization/Localizable.xcstrings` on main.
These were found on a copy fetched 2026-09-17; anything fixed since should be dropped from the list.

**Title:** Placeholder and plural problems in Localizable.xcstrings (pl, uk, be, ca, ko)

**Body:**

Hi, I ran a placeholder/plural checker over `Localizable.xcstrings` and it flagged a few things that look like real bugs in shipped translations. Sharing in case it's useful; happy to open a PR for any of these if you'd like.

**Placeholders**

- `%@ add-tag-groups.edit.title.field.warning.already-exists`, Polish: `już istnieje`. The `%@` was dropped, so the tag name never shows.
- `instance.list.posts-%@`, Catalan: `% publicacions`. The `@` is missing, so it renders a literal `%`.
- `account.label.followers %lld %@`, Korean: `팔로워 %2$@명`. The source uses the `%#@followers@` plural variable; the Korean string references `%2$@` directly, so the plural substitution is bypassed.

**Missing plural forms (Polish, Ukrainian, Belarusian)**

These three languages need `few` and `many` alongside `one` and `other` (CLDR). 44 plural variations in the catalog have only `one`/`other`, so counts like 2, 3, 4 and 5+ pick the wrong form, e.g. "2 posty" comes out as the `other` form. Full list:

pl:
  - accessibility.tabs.timeline.unread-posts.label-%lld · missing many
  - account.detail.featured-tags-n-posts %lld · missing many
  - account.detail.n-fields %lld · missing many
  - account.label.followers %lld %@ (substitution followers) · missing few, many
  - account.post.pinned %lld · missing many
  - design.tag.n-posts-from-n-participants %lld %lld (substitution count_participants) · missing few, many
  - design.tag.n-posts-from-n-participants %lld %lld (substitution count_posts) · missing many
  - notifications-others-count %lld · missing many
  - status.poll.n-votes %lld · missing many
  - status.poll.n-votes-voters %lld %lld (substitution count_voters) · missing few, many
  - status.poll.n-votes-voters %lld %lld (substitution count_votes) · missing many
  - status.summary.n-boosts %lld · missing many
  - status.summary.n-favorites %lld · missing many
  - status.summary.n-replies %lld · missing few, many
  - tag.suggested.mentions-%lld · missing many
  - timeline-new-posts %lld · missing many
  - timeline.n-recent-from-n-participants %lld %lld (substitution count_participants) · missing few, many
  - timeline.n-recent-from-n-participants %lld %lld (substitution count_posts) · missing many

uk:
  - accessibility.tabs.timeline.unread-posts.label-%lld · missing few, many
  - account.detail.featured-tags-n-posts %lld · missing few, many
  - account.detail.n-fields %lld · missing few, many
  - account.label.followers %lld %@ (substitution followers) · missing few, many
  - account.post.pinned %lld · missing many
  - notifications-others-count %lld · missing few, many
  - status.poll.n-votes %lld · missing few, many
  - status.summary.n-boosts %lld · missing few, many
  - status.summary.n-favorites %lld · missing few, many
  - status.summary.n-replies %lld · missing few, many
  - tag.suggested.mentions-%lld · missing few, many
  - timeline-new-posts %lld · missing few, many
  - trending-tag-people-talking %lld · missing few, many

be:
  - accessibility.tabs.timeline.unread-posts.label-%lld · missing few, many
  - account.detail.featured-tags-n-posts %lld · missing few, many
  - account.detail.n-fields %lld · missing few, many
  - account.label.followers %lld %@ (substitution followers) · missing few, many
  - account.post.pinned %lld · missing many
  - notifications-others-count %lld · missing few, many
  - status.poll.n-votes %lld · missing few, many
  - status.summary.n-boosts %lld · missing few, many
  - status.summary.n-favorites %lld · missing few, many
  - status.summary.n-replies %lld · missing few, many
  - tag.suggested.mentions-%lld · missing few, many
  - timeline-new-posts %lld · missing few, many
  - trending-tag-people-talking %lld · missing few, many

Found with [polygo](https://github.com/Na5co/polygo) (`polygo check`), a small open-source CLI I'm working on. Thanks for Ice Cubes!
