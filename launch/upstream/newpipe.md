# Issue for TeamNewPipe/NewPipe

NewPipe takes translations from Weblate; file as an issue (or fix on hosted.weblate.org/projects/newpipe).

**Title:** A few translations with a dropped or malformed placeholder

**Body:**

Ran a placeholder check over `app/src/main/res` and found these, checked by hand against `values/strings.xml`:

- `sign_in_confirm_not_bot_error` in bs, es, eu, ka, sr: the English string uses `%1$s` twice and `%2$s` once; these translations only have `%1$s`, so the second argument never shows.
- `did_you_mean` in ckb: `٪1$s` uses the Arabic percent sign (U+066A) instead of `%`, so the query is never substituted and users see the literal text.
- `progressive_load_interval_summary` in ckb: `%1$s` missing.
- `export_subscriptions` in vi, `other` quantity: `%1$d` missing.

I know translations live on Weblate, so this is just a heads-up for whoever maintains those languages. Happy to fix them there if that's preferred.
