# polygo extract

For a web app whose UI text still lives in the code. `extract` builds `locales/en.json` from the markup; `--rewrite` swaps the strings for `t("key")` calls and generates the helper.

```sh
polygo extract --dry-run   # lists every piece of UI text it found, with file:line
polygo extract --rewrite   # writes locales/en.json, rewrites the code, generates src/i18n.ts
polygo init && polygo add de fr && polygo translate
```

`extract` reads the markup in JSX, HTML inside template literals, and `.html`/`.vue`/`.svelte` files: text between tags plus `placeholder`, `title`, `alt` and `aria-label` attributes. It skips `<script>`, `<style>`, `<svg>`, `<code>`, tests and `node_modules`. `${expr}` and `{expr}` become `{{0}}` placeholders and are passed as arguments: `<p>Signed in as ${email}.</p>` becomes `<p>${t("Signed in as {{0}}.", { 0: email })}</p>`.

`--rewrite` edits `.ts`/`.tsx`/`.js`/`.jsx` files in place, adds the import, and generates a 20-line `i18n.ts` with `t`, `setLocale` and `addCatalog` (no library needed; keys are the English text, so English works with nothing loaded). Review it with `git diff`. Sentences split by inline markup (`writes a <code>.form</code> file`) are listed as fragments and left alone, because translating the pieces separately gives bad results. On a 160-file server-rendered app it rewrote 387 strings in 20 files with no new TypeScript errors.

Keep brand names and whole pages out of it with flags or `polygo.toml`:

```toml
[extract]
ignore = ["Leafslip", "API key"]          # strings containing these are skipped
ignore_paths = ["src/admin*", "legacy/**"]
```

iOS, Android, Flutter and gettext already have their own extractors, so `extract` is for the web.
