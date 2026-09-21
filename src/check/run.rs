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
    let directive_skip: std::collections::BTreeSet<(usize, String)> =
        project::directive_skipped_keys(root, cfg)?
            .into_iter()
            .collect();
    let skipped = |idx: usize, spec: &crate::config::FileSpec, key: &str| {
        skip.matches(&spec.path, key)
            || directive_skip.contains(&(idx, key.split('#').next().unwrap_or(key).to_string()))
    };
    let lock = crate::lockfile::Lock::load(&root.join(crate::lockfile::FILE_NAME))?;

    let mut locator = crate::check::locate::Locator::new(root);
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
            if let Some(m) = placeholders::compare(&u.source, t) {
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

    // Format-level plural structures.
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
                let escapes =
                    |doc: &formats::android::Document| -> Vec<(String, &'static str, String)> {
                        let mut out = Vec::new();
                        for e in &doc.entries {
                            if !e.translatable || skipped(idx, spec, &e.name) {
                                continue;
                            }
                            for v in &e.values {
                                if let Some((sev, m)) = android_escape_problem(&v.raw) {
                                    out.push((e.name.clone(), sev, m));
                                }
                            }
                        }
                        out
                    };
                let source = formats::android::parse(&crate::formats::read_text(&path)?)
                    .with_context(|| format!("parsing {}", path.display()))?;
                for (name, severity, m) in escapes(&source) {
                    let (file, line) = locator.locate(spec, &name, &cfg.source_locale);
                    report.push(Finding {
                        file,
                        line,
                        key: name,
                        locale: cfg.source_locale.clone(),
                        code: "escape",
                        severity,
                        message: m,
                    });
                }
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
                    for (name, severity, m) in escapes(&doc) {
                        let (file, line) = locator.locate(spec, &name, locale);
                        report.push(Finding {
                            file,
                            line,
                            key: name,
                            locale: locale.clone(),
                            code: "escape",
                            severity,
                            message: m,
                        });
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
            Format::Arb | Format::Po | Format::Resx | Format::Strings => {}
        }
        // Keys a locale file has and the source does not: dead translations that
        // accumulate forever. And gettext's fuzzy flag, which ships as untranslated.
        if let Some(template) = &spec.locale_path {
            let text = crate::formats::read_text(&path)?;
            let source_keys = keys_of(spec.format, &text)?;
            for locale in &locales {
                let lp = root.join(project::locale_file(template, locale));
                if !lp.exists() {
                    continue;
                }
                let ltext = crate::formats::read_text(&lp)?;
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
