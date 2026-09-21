//! Run every validator over a project and collect findings.

use crate::check::{placeholders, plurals, text};
use crate::config::{Config, Format};
use crate::formats;
use crate::project;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    /// The file a fix would be made in: the locale file for per-locale formats, the
    /// catalog for `.xcstrings`.
    pub file: String,
    /// Line of the key in `file`, when it could be found there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub key: String,
    pub locale: String,
    pub code: &'static str,
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub errors: usize,
    pub warnings: usize,
    /// Translations that were looked at (unit × locale with a value).
    pub checked: usize,
    /// locale → (translated, applicable units): what `status` would say is missing.
    pub coverage: BTreeMap<String, (usize, usize)>,
}

impl Report {
    fn push(&mut self, f: Finding) {
        if f.severity == "error" {
            self.errors += 1;
        } else {
            self.warnings += 1;
        }
        self.findings.push(f);
    }

    /// Keys with errors, per locale: what `--fix` re-translates.
    pub fn error_keys(&self) -> BTreeMap<String, Vec<String>> {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in &self.findings {
            if f.severity == "error" && f.code != "plural" {
                let v = m.entry(f.locale.clone()).or_default();
                if !v.contains(&f.key) {
                    v.push(f.key.clone());
                }
            }
        }
        m
    }
}

pub struct Options {
    pub locales: Option<Vec<String>>,
    pub length_ratio: f64,
}

pub fn run(root: &Path, cfg: &Config, opts: &Options) -> Result<Report> {
    let mut report = Report::default();
    let locales: Vec<String> = opts
        .locales
        .clone()
        .unwrap_or_else(|| cfg.target_locales.clone());
    let units = project::load_units(root, cfg)?;
    let skip = cfg.key_skip()?;
    // `polygo:skip` in a developer comment, for the format-level checks below that read
    // files directly (units already exclude these keys).
    let (directive_skipped, directive_ignores) = project::directive_rules(root, cfg)?;
    let directive_skip: std::collections::BTreeSet<(usize, String)> =
        directive_skipped.into_iter().collect();
    let skipped = |idx: usize, spec: &crate::config::FileSpec, key: &str| {
        skip.matches(&spec.path, key)
            || directive_skip.contains(&(idx, key.split('#').next().unwrap_or(key).to_string()))
    };
    let lock = crate::lockfile::Lock::load(&root.join(crate::lockfile::FILE_NAME))?;

    let mut locator = crate::check::locate::Locator::new(root);
    let glossary = crate::glossary::load(root, cfg)?;
    for locale in &locales {
        let total = units.iter().filter(|u| u.applies_to(locale)).count();
        let done = units
            .iter()
            .filter(|u| u.applies_to(locale) && u.translations.contains_key(locale))
            .count();
        report.coverage.insert(locale.clone(), (done, total));
    }

    // Per-unit text + placeholder checks on every existing translation. `(code, severity,
    // message)` are collected first and located once, since locating reads the file.
    for u in &units {
        let (idx, local_key) = project::split_key(cfg, &u.key);
        for locale in &locales {
            let Some(t) = u.translations.get(locale) else {
                continue;
            };
            report.checked += 1;
            let mut found: Vec<(&'static str, &'static str, String)> = Vec::new();
            if let Some(max) = crate::core::directives(u.comment.as_deref()).max_chars
                && t.chars().count() > max
            {
                found.push((
                    "length",
                    "error",
                    format!(
                        "{} chars, but the key allows at most {max} (polygo:max)",
                        t.chars().count()
                    ),
                ));
            }
            // Plural forms: an exact-count form may leave the number out, a Russian `one`
            // (also 21, 31…) may not.
            let mismatch = match crate::core::split_plural(&u.key) {
                Some((_, cat)) => plurals::compare_forms(locale, cat, &u.source, t),
                None => placeholders::compare(&u.source, t),
            };
            if let Some(m) = mismatch {
                found.push(("placeholders", "error", m.to_string()));
            }
            // A translation polygo wrote and confirmed (recorded in the lockfile) is not
            // re-flagged as identical: the model was asked twice and kept it.
            let confirmed = lock
                .keys
                .get(&u.key)
                .and_then(|r| r.locales.get(locale))
                .is_some_and(|l| l.hash == crate::core::hash(t));
            for f in text::check_text(&u.source, t, &cfg.source_locale, locale, opts.length_ratio) {
                if f.code == "identical" && confirmed {
                    continue;
                }
                let sev = match f.severity {
                    text::Severity::Error => "error",
                    text::Severity::Warning => "warning",
                };
                found.push((f.code, sev, f.message));
            }
            for (arg, cats) in plurals::icu_cases(t) {
                let missing = plurals::missing(locale, &cats);
                if !missing.is_empty() {
                    found.push((
                        "plural",
                        "error",
                        format!("ICU plural `{arg}` is missing {}", missing.join(", ")),
                    ));
                }
            }
            for v in glossary.violations(locale, &u.source, t) {
                found.push(("glossary", "error", v));
            }
            if let Some(m) = crate::check::content::mojibake(t) {
                found.push(("encoding", "warning", m));
            }
            if let Some(m) = crate::check::content::invisible(t) {
                found.push(("invisible", "warning", m));
            }
            if let Some(m) = crate::check::content::link_mismatch(&u.source, t) {
                found.push(("link", "warning", m));
            }
            if let Some(m) = crate::check::content::unbalanced(&u.source, t) {
                found.push(("brackets", "warning", m));
            }
            if let Some(m) = crate::check::content::double_escaped(t) {
                found.push(("entities", "warning", m));
            }
            if found.is_empty() {
                continue;
            }
            let (file, line) = locator.locate(&cfg.files[idx], local_key, locale);
            for (code, severity, message) in found {
                report.push(Finding {
                    file: file.clone(),
                    line,
                    key: u.key.clone(),
                    locale: locale.clone(),
                    code,
                    severity,
                    message,
                });
            }
        }
    }

    // The same short source text translated two ways in one locale ("Cancel" →
    // "Abbrechen" here, "Abbruch" there): the minority gets the warning.
    for locale in &locales {
        let mut by_source: BTreeMap<&str, Vec<(&crate::core::Unit, &str)>> = BTreeMap::new();
        for u in &units {
            if crate::core::split_plural(&u.key).is_some() || !u.applies_to(locale) {
                continue;
            }
            let Some(t) = u.translations.get(locale) else {
                continue;
            };
            let src = u.source.trim();
            // UI terms only: a few words, no placeholders or markup, not a sentence.
            let words = src.split_whitespace().count();
            if words == 0
                || words > 3
                || src.contains(['%', '{', '<', '$'])
                || src.chars().filter(|c| c.is_alphabetic()).count() < 3
            {
                continue;
            }
            // An untranslated copy is the `identical` warning's business, not a vote.
            if t.trim() == src {
                continue;
            }
            by_source.entry(src).or_default().push((u, t.trim()));
        }
        for (src, group) in by_source {
            if group.len() < 2 {
                continue;
            }
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut shown: BTreeMap<String, &str> = BTreeMap::new();
            for (_, t) in &group {
                let l = t.to_lowercase();
                *counts.entry(l.clone()).or_default() += 1;
                shown.entry(l).or_insert(t);
            }
            if counts.len() < 2 {
                continue;
            }
            let mut ranked: Vec<(&String, &usize)> = counts.iter().collect();
            ranked.sort_by_key(|(t, n)| (std::cmp::Reverse(**n), (*t).clone()));
            let (majority, n) = (ranked[0].0.clone(), *ranked[0].1);
            // A tie is a choice, not a mistake.
            if ranked.get(1).is_some_and(|(_, m)| **m == n) {
                continue;
            }
            for (u, t) in &group {
                let tl = t.to_lowercase();
                if tl == majority || inflection_of(&tl, &majority) {
                    continue;
                }
                let (idx, local_key) = project::split_key(cfg, &u.key);
                let (file, line) = locator.locate(&cfg.files[idx], local_key, locale);
                report.push(Finding {
                    file,
                    line,
                    key: u.key.clone(),
                    locale: locale.clone(),
                    code: "inconsistent",
                    severity: "warning",
                    message: format!(
                        "`{src}` is `{}` in {n} other key(s), here `{t}`",
                        shown.get(&majority).copied().unwrap_or(majority.as_str())
                    ),
                });
            }
        }
    }

    // Format-level plural structures.
    let mut seen_dicts: std::collections::BTreeSet<std::path::PathBuf> = Default::default();
    for (idx, spec) in cfg.files.iter().enumerate() {
        let path = root.join(&spec.path);
        match spec.format {
            Format::Xcstrings => {
                let doc = formats::xcstrings::parse(&crate::formats::read_text(&path)?)?;
                // Xcode's own bookkeeping: a unit marked needs_review / stale, or a key
                // whose extractionState is stale (no longer found in the code).
                for (key, locale, state) in xcstrings_states(&doc, &cfg.source_locale) {
                    if skipped(idx, spec, &key)
                        || (!locale.is_empty() && !locales.contains(&locale))
                    {
                        continue;
                    }
                    let (file, line) = locator.locate(spec, &key, &locale);
                    let (locale, message) = if locale.is_empty() {
                        (
                            cfg.source_locale.clone(),
                            "extractionState is stale: Xcode no longer finds this key in the code"
                                .to_string(),
                        )
                    } else {
                        (locale, format!("marked `{state}` in Xcode, shipped as is"))
                    };
                    report.push(Finding {
                        file,
                        line,
                        key,
                        locale,
                        code: "state",
                        severity: "warning",
                        message,
                    });
                }
                for (key, locale, cats) in plurals::xcstrings_plurals(&doc) {
                    if !locales.contains(&locale) || skipped(idx, spec, &key) {
                        continue;
                    }
                    let missing = plurals::missing(&locale, &cats);
                    if !missing.is_empty() {
                        let (file, line) = locator.locate(spec, &key, &locale);
                        report.push(Finding {
                            file,
                            line,
                            key,
                            locale,
                            code: "plural",
                            severity: "error",
                            message: format!("missing plural form(s) {}", missing.join(", ")),
                        });
                    }
                }
            }
            Format::Android => {
                // What aapt rejects at build time: an apostrophe or a double quote that
                // is not escaped, a leading @ or ? (a resource reference). The source
                // file breaks the build exactly like a translation does.
                let escapes = |doc: &formats::android::Document| -> Vec<(
                    String,
                    &'static str,
                    &'static str,
                    String,
                )> {
                    let mut out = Vec::new();
                    for e in &doc.entries {
                        if !e.translatable || skipped(idx, spec, &e.name) {
                            continue;
                        }
                        for v in &e.values {
                            if let Some((sev, m)) = android_escape_problem(&v.raw) {
                                out.push((e.name.clone(), "escape", sev, m));
                            }
                            if e.formatted
                                && let Some(m) =
                                    crate::check::content::android_unnumbered_args(&v.text())
                            {
                                out.push((e.name.clone(), "placeholders", "error", m));
                            }
                        }
                    }
                    out
                };
                let source = formats::android::parse(&crate::formats::read_text(&path)?)
                    .with_context(|| format!("parsing {}", path.display()))?;
                for (name, code, severity, m) in escapes(&source) {
                    let (file, line) = locator.locate(spec, &name, &cfg.source_locale);
                    report.push(Finding {
                        file,
                        line,
                        key: name,
                        locale: cfg.source_locale.clone(),
                        code,
                        severity,
                        message: m,
                    });
                }
                let array_len = |doc: &formats::android::Document| -> BTreeMap<String, usize> {
                    doc.entries
                        .iter()
                        .filter(|e| e.kind == formats::android::Kind::StringArray)
                        .map(|e| (e.name.clone(), e.values.len()))
                        .collect()
                };
                let source_arrays = array_len(&source);
                let Some(template) = &spec.locale_path else {
                    continue;
                };
                for locale in &locales {
                    let lp = root.join(project::locale_file(template, locale));
                    if !lp.exists() {
                        continue;
                    }
                    let doc = formats::android::parse(&crate::formats::read_text(&lp)?)
                        .with_context(|| format!("parsing {}", lp.display()))?;
                    for (name, code, severity, m) in escapes(&doc) {
                        let (file, line) = locator.locate(spec, &name, locale);
                        report.push(Finding {
                            file,
                            line,
                            key: name,
                            locale: locale.clone(),
                            code,
                            severity,
                            message: m,
                        });
                    }
                    // Arrays are positional: a shorter one is an IndexOutOfBounds at
                    // runtime, a longer one shows items the source never had.
                    for (name, n) in array_len(&doc) {
                        if skipped(idx, spec, &name) {
                            continue;
                        }
                        if let Some(&src_n) = source_arrays.get(&name)
                            && src_n != n
                        {
                            let (file, line) = locator.locate(spec, &name, locale);
                            report.push(Finding {
                                file,
                                line,
                                key: name,
                                locale: locale.clone(),
                                code: "array",
                                severity: "error",
                                message: format!("{n} item(s) vs {src_n} in the source"),
                            });
                        }
                    }
                    for (name, cats) in plurals::android_plurals(&doc) {
                        if skipped(idx, spec, &name) {
                            continue;
                        }
                        let missing = plurals::missing(locale, &cats);
                        if !missing.is_empty() {
                            let (file, line) = locator.locate(spec, &name, locale);
                            report.push(Finding {
                                file,
                                line,
                                key: name,
                                locale: locale.clone(),
                                code: "plural",
                                severity: "error",
                                message: format!("missing plural form(s) {}", missing.join(", ")),
                            });
                        }
                    }
                }
            }
            Format::Json => {
                let Some(template) = &spec.locale_path else {
                    continue;
                };
                let source = formats::json::parse(&crate::formats::read_text(&path)?)?;
                let source_groups = plurals::i18next_plural_groups(
                    &source.entries.iter().map(|e| e.key()).collect::<Vec<_>>(),
                );
                for locale in &locales {
                    let lp = root.join(project::locale_file(template, locale));
                    if !lp.exists() {
                        continue;
                    }
                    let doc = formats::json::parse(&crate::formats::read_text(&lp)?)
                        .with_context(|| format!("parsing {}", lp.display()))?;
                    let groups = plurals::i18next_groups(
                        &doc.entries.iter().map(|e| e.key()).collect::<Vec<_>>(),
                    );
                    for base in source_groups.keys() {
                        if skipped(idx, spec, base) {
                            continue;
                        }
                        let cats = groups.get(base).cloned().unwrap_or_default();
                        let missing = plurals::missing(locale, &cats);
                        if !missing.is_empty() {
                            let (file, line) =
                                locator.locate(spec, &format!("{base}_other"), locale);
                            report.push(Finding {
                                file,
                                line,
                                key: base.clone(),
                                locale: locale.clone(),
                                code: "plural",
                                severity: "error",
                                message: format!(
                                    "i18next plural keys missing: {}",
                                    missing
                                        .iter()
                                        .map(|c| format!("{base}_{c}"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                            });
                        }
                    }
                }
            }
            Format::Strings => {
                // Every .stringsdict in the source .lproj (any name: Signal ships
                // PluralAware.stringsdict): each plural variable needs the locale's CLDR
                // categories, every form must keep the placeholders of the source's form,
                // and a key the locale's dict lacks falls back to English. One .lproj may
                // hold several .strings specs; its dicts are checked once.
                let Some(src_dir) = path.parent() else {
                    continue;
                };
                let Some(template) = &spec.locale_path else {
                    continue;
                };
                let mut dicts: Vec<std::path::PathBuf> = std::fs::read_dir(src_dir)
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| e.path())
                            .filter(|p| p.extension().is_some_and(|e| e == "stringsdict"))
                            .collect()
                    })
                    .unwrap_or_default();
                dicts.sort();
                for src_dict in dicts {
                    if !seen_dicts.insert(src_dict.clone()) {
                        continue;
                    }
                    let source =
                        formats::stringsdict::parse(&crate::formats::read_text(&src_dict)?)
                            .with_context(|| format!("parsing {}", src_dict.display()))?;
                    let name = src_dict.file_name().unwrap().to_owned();
                    for locale in &locales {
                        let lp = root
                            .join(project::locale_file(template, locale))
                            .with_file_name(&name);
                        let rel = lp
                            .strip_prefix(root)
                            .unwrap_or(&lp)
                            .display()
                            .to_string()
                            .replace('\\', "/");
                        if !lp.exists() {
                            continue;
                        }
                        let doc = formats::stringsdict::parse(&crate::formats::read_text(&lp)?)
                            .with_context(|| format!("parsing {}", lp.display()))?;
                        for (key, se) in &source.entries {
                            if skipped(idx, spec, key) {
                                continue;
                            }
                            let Some(te) = doc.entries.get(key) else {
                                report.push(Finding {
                                file: rel.clone(),
                                line: None,
                                key: key.clone(),
                                locale: locale.clone(),
                                code: "plural",
                                severity: "warning",
                                message: "not in this locale's .stringsdict: iOS falls back to the source language".into(),
                            });
                                continue;
                            };
                            for (var, sv) in &se.variables {
                                let Some(tv) = te.variables.get(var) else {
                                    report.push(Finding {
                                        file: rel.clone(),
                                        line: Some(te.line),
                                        key: format!("{key}#{var}"),
                                        locale: locale.clone(),
                                        code: "plural",
                                        severity: "error",
                                        message: format!("variable `{var}` is missing"),
                                    });
                                    continue;
                                };
                                let cats: Vec<String> =
                                    tv.forms.iter().map(|(c, _)| c.clone()).collect();
                                let missing = plurals::missing(locale, &cats);
                                if !missing.is_empty() {
                                    report.push(Finding {
                                        file: rel.clone(),
                                        line: Some(te.line),
                                        key: format!("{key}#{var}"),
                                        locale: locale.clone(),
                                        code: "plural",
                                        severity: "error",
                                        message: format!(
                                            "missing plural form(s) {}",
                                            missing.join(", ")
                                        ),
                                    });
                                }
                                let src_other = sv
                                    .forms
                                    .iter()
                                    .find(|(c, _)| c == "other")
                                    .or(sv.forms.last())
                                    .map(|(_, t)| t.as_str())
                                    .unwrap_or("");
                                // A variable is referenced from the format key or from
                                // another variable's forms (`%d %2$#@total@`).
                                let refs: String = std::iter::once(se.format.as_str())
                                    .chain(
                                        se.variables
                                            .values()
                                            .flat_map(|v| v.forms.iter().map(|(_, f)| f.as_str())),
                                    )
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                let pos = plurals::stringsdict_position(&refs, var);
                                for (cat, form) in &tv.forms {
                                    let src_form = sv
                                        .forms
                                        .iter()
                                        .find(|(c, _)| c == cat)
                                        .map_or(src_other, |(_, t)| t.as_str());
                                    // Inside a variable `%d` is that variable's argument;
                                    // in an exact-count form the number may be left out.
                                    let (s, t) = (
                                        plurals::renumber(src_form, pos),
                                        plurals::renumber(form, pos),
                                    );
                                    if let Some(m) = plurals::compare_forms(locale, cat, &s, &t) {
                                        report.push(Finding {
                                            file: rel.clone(),
                                            line: Some(te.line),
                                            key: format!("{key}#{var}.{cat}"),
                                            locale: locale.clone(),
                                            code: "placeholders",
                                            severity: "error",
                                            message: m.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Format::Arb | Format::Po | Format::Resx => {}
        }
        // The same key twice in one file: the last one wins silently, whichever the
        // translator meant.
        let text = crate::formats::read_text(&path)?;
        for key in duplicate_keys(spec.format, &text)? {
            if skipped(idx, spec, &key) {
                continue;
            }
            let (file, line) = locator.locate(spec, &key, &cfg.source_locale);
            report.push(Finding {
                file,
                line,
                key,
                locale: cfg.source_locale.clone(),
                code: "duplicate",
                severity: "error",
                message: "key appears more than once in this file".into(),
            });
        }
        // Keys a locale file has and the source does not: dead translations that
        // accumulate forever. And gettext's fuzzy flag, which ships as untranslated.
        if let Some(template) = &spec.locale_path {
            let source_keys = keys_of(spec.format, &text)?;
            for locale in &locales {
                let lp = root.join(project::locale_file(template, locale));
                if !lp.exists() {
                    continue;
                }
                let ltext = crate::formats::read_text(&lp)?;
                for key in duplicate_keys(spec.format, &ltext)? {
                    if skipped(idx, spec, &key) {
                        continue;
                    }
                    let (file, line) = locator.locate(spec, &key, locale);
                    report.push(Finding {
                        file,
                        line,
                        key,
                        locale: locale.clone(),
                        code: "duplicate",
                        severity: "error",
                        message: "key appears more than once in this file".into(),
                    });
                }
                for key in keys_of(spec.format, &ltext)? {
                    if source_keys.contains(&key) || skipped(idx, spec, &key) {
                        continue;
                    }
                    // `photos_few` next to a source `photos_one`/`photos_other` is a
                    // plural form the locale needs, not an orphan.
                    if spec.format == Format::Json
                        && let Some((base, _)) = plurals::i18next_split(&key)
                        && source_keys.contains(&format!("{base}_other"))
                    {
                        continue;
                    }
                    let (file, line) = locator.locate(spec, &key, locale);
                    report.push(Finding {
                        file,
                        line,
                        key,
                        locale: locale.clone(),
                        code: "orphan",
                        severity: "warning",
                        message: "not in the source file: delete it, or restore the source key"
                            .into(),
                    });
                }
                if spec.format == Format::Po {
                    let doc = formats::po::parse(&ltext)?;
                    for e in doc
                        .entries
                        .iter()
                        .filter(|e| e.fuzzy && !e.msgid.is_empty())
                    {
                        let key = e.key();
                        if skipped(idx, spec, &key) {
                            continue;
                        }
                        let (file, line) = locator.locate(spec, &key, locale);
                        report.push(Finding {
                            file,
                            line,
                            key,
                            locale: locale.clone(),
                            code: "fuzzy",
                            severity: "warning",
                            message: "marked fuzzy: gettext shows the source text instead; review it and drop the flag".into(),
                        });
                    }
                }
            }
        }
    }
    // `polygo:ignore=code` in a comment and `[keys] ignore` in polygo.toml: quiet, per key
    // and code, without hiding the key from everything the way `skip` does.
    let ignore_globs: Vec<(globset::GlobSet, Vec<String>)> = cfg
        .keys
        .ignore
        .iter()
        .map(|(g, codes)| {
            let mut b = globset::GlobSetBuilder::new();
            b.add(
                globset::Glob::new(g)
                    .with_context(|| format!("[keys] ignore: bad glob {g:?} in polygo.toml"))?,
            );
            Ok((b.build()?, codes.clone()))
        })
        .collect::<Result<_>>()?;
    if !ignore_globs.is_empty() || !directive_ignores.is_empty() {
        report.findings.retain(|f| {
            let (idx, local) = project::split_key(cfg, &f.key);
            let base = local.split('#').next().unwrap_or(local);
            let code = f.code.to_string();
            let by_comment = directive_ignores
                .get(&(idx, base.to_string()))
                .is_some_and(|codes| codes.contains(&code));
            let path = cfg.files[idx].path.display().to_string().replace('\\', "/");
            let by_glob = ignore_globs.iter().any(|(set, codes)| {
                codes.contains(&code)
                    && (set.is_match(base) || set.is_match(format!("{path}:{base}")))
            });
            !(by_comment || by_glob)
        });
        report.errors = report
            .findings
            .iter()
            .filter(|f| f.severity == "error")
            .count();
        report.warnings = report.findings.len() - report.errors;
    }
    report.findings.sort_by(|a, b| {
        (&a.file, &a.locale, &a.key, a.code).cmp(&(&b.file, &b.locale, &b.key, b.code))
    });
    Ok(report)
}

/// Android resource strings that `aapt` refuses: an unescaped `'` (must be `\'` or the
/// whole value wrapped in `"…"`), an unescaped `"` in an unquoted value, or a value
/// starting with `@` / `?`, which is read as a resource reference. Tags, CDATA and
/// entities are left alone.
pub fn android_escape_problem(raw: &str) -> Option<(&'static str, String)> {
    let t = raw.trim();
    if t.starts_with("<![CDATA[") {
        return None;
    }
    // `@string/name`, `?attr/name`, `@android:string/ok` are references and fine; any
    // other leading @ or ? makes aapt look for a resource that does not exist.
    if (t.starts_with('@') || t.starts_with('?')) && !is_android_reference(t) {
        return Some((
            "error",
            format!(
                "starts with `{}`: Android reads that as a resource reference; write `\\{}`",
                &t[..1],
                &t[..1]
            ),
        ));
    }
    let quoted = t.len() >= 2 && t.starts_with('"') && t.ends_with('"');
    if quoted {
        return None;
    }
    let b = t.as_bytes();
    let mut i = 0;
    let mut in_tag = false;
    let mut dq = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'<' => in_tag = true,
            b'>' => in_tag = false,
            b'\'' if !in_tag => {
                return Some((
                    "error",
                    "unescaped apostrophe: Android needs `\\'` (or wrap the whole string in double quotes)"
                        .into(),
                ));
            }
            b'"' if !in_tag => dq += 1,
            _ => {}
        }
        i += 1;
    }
    // An unescaped " toggles whitespace mode and is dropped from the text; a stray one
    // silently disappears from the UI (DuckDuckGo and NewPipe both ship one).
    if dq % 2 == 1 {
        return Some((
            "warning",
            "stray double quote: Android drops it from the text; escape it as `\\\"`".into(),
        ));
    }
    None
}

fn is_android_reference(t: &str) -> bool {
    let body = &t[1..];
    let body = body.strip_prefix('+').unwrap_or(body);
    let (pkg_type, name) = match body.split_once('/') {
        Some(x) => x,
        None => return false,
    };
    let ty = pkg_type.rsplit(':').next().unwrap_or(pkg_type);
    !name.is_empty()
        && ty.chars().all(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Every key a file defines, for the orphan check.
fn keys_of(format: Format, text: &str) -> Result<std::collections::BTreeSet<String>> {
    Ok(match format {
        Format::Json => formats::json::parse(text)?
            .entries
            .iter()
            .map(|e| e.key())
            .collect(),
        Format::Android => formats::android::parse(text)?
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect(),
        Format::Arb => formats::arb::values(&formats::arb::parse(text)?)
            .into_keys()
            .collect(),
        Format::Po => formats::po::parse(text)?
            .entries
            .iter()
            .filter(|e| !e.msgid.is_empty())
            .map(|e| e.key())
            .collect(),
        Format::Resx => formats::resx::values(&formats::resx::parse(text)?)
            .into_keys()
            .collect(),
        Format::Strings => formats::strings::parse(text)?
            .entries
            .iter()
            .map(|e| e.key.clone())
            .collect(),
        Format::Xcstrings => Default::default(),
    })
}

/// `(key, locale, state)` for every string unit in a target locale whose state is not
/// `translated`, and `(key, "", "stale")` for keys with `extractionState = stale`.
fn xcstrings_states(
    doc: &formats::xcstrings::Document,
    source_locale: &str,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Some(strings) = doc.root.get("strings").and_then(|v| v.as_object()) else {
        return out;
    };
    for (key, entry) in strings {
        if entry.get("extractionState").and_then(|v| v.as_str()) == Some("stale") {
            out.push((key.clone(), String::new(), "stale".into()));
        }
        let Some(locs) = entry.get("localizations").and_then(|v| v.as_object()) else {
            continue;
        };
        for (locale, l) in locs {
            if locale == source_locale {
                continue;
            }
            let mut states = Vec::new();
            if let Some(s) = l.pointer("/stringUnit/state").and_then(|v| v.as_str()) {
                states.push(s);
            }
            if let Some(p) = l.pointer("/variations/plural").and_then(|v| v.as_object()) {
                states.extend(
                    p.values()
                        .filter_map(|c| c.pointer("/stringUnit/state").and_then(|v| v.as_str())),
                );
            }
            if let Some(s) = states
                .into_iter()
                .find(|s| matches!(*s, "needs_review" | "stale" | "new"))
            {
                out.push((key.clone(), locale.clone(), s.to_string()));
            }
        }
    }
    out
}

/// Keys defined more than once in one file, in first-seen order.
fn duplicate_keys(format: Format, text: &str) -> Result<Vec<String>> {
    let all: Vec<String> = match format {
        Format::Json | Format::Arb => formats::json::parse(text)?
            .entries
            .iter()
            .map(|e| e.key())
            .collect(),
        // A string and a plurals may share a name (different resource types).
        Format::Android => formats::android::parse(text)?
            .entries
            .iter()
            .map(|e| format!("{:?}\u{0}{}", e.kind, e.name))
            .collect(),
        Format::Po => formats::po::parse(text)?
            .entries
            .iter()
            .filter(|e| !e.msgid.is_empty())
            .map(|e| e.key())
            .collect(),
        Format::Resx => formats::resx::parse(text)?
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect(),
        Format::Strings => formats::strings::parse(text)?
            .entries
            .iter()
            .map(|e| e.key.clone())
            .collect(),
        // serde_json keeps the last of duplicate keys; a String Catalog written by Xcode
        // never has them.
        Format::Xcstrings => vec![],
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut dups = Vec::new();
    for k in all {
        if !seen.insert(k.clone()) && !dups.contains(&k) {
            dups.push(k);
        }
    }
    Ok(dups
        .into_iter()
        .map(|k| k.rsplit('\u{0}').next().unwrap_or(&k).to_string())
        .collect())
}

/// `aucune` vs `aucun`, `attiva` vs `attivo`, `essayer` vs `essayez`: the same word
/// agreeing with a different noun or mood, not a different translation.
fn inflection_of(a: &str, b: &str) -> bool {
    let common = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    let shorter = a.chars().count().min(b.chars().count());
    common >= 3 && common + 2 >= shorter && a.chars().count().abs_diff(b.chars().count()) <= 2
}
