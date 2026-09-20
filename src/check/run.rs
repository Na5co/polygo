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
    pub file: String,
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
    let lock = crate::lockfile::Lock::load(&root.join(crate::lockfile::FILE_NAME))?;

    // Per-unit text + placeholder checks on every existing translation.
    for u in &units {
        let (idx, _) = project::split_key(cfg, &u.key);
        let file = cfg.files[idx].path.display().to_string();
        for locale in &locales {
            let Some(t) = u.translations.get(locale) else {
                continue;
            };
            report.checked += 1;
            if let Some(max) = crate::core::directives(u.comment.as_deref()).max_chars
                && t.chars().count() > max
            {
                report.push(Finding {
                    file: file.clone(),
                    key: u.key.clone(),
                    locale: locale.clone(),
                    code: "length",
                    severity: "error",
                    message: format!(
                        "{} chars, but the key allows at most {max} (polygo:max)",
                        t.chars().count()
                    ),
                });
            }
            if let Some(m) = placeholders::compare(&u.source, t) {
                report.push(Finding {
                    file: file.clone(),
                    key: u.key.clone(),
                    locale: locale.clone(),
                    code: "placeholders",
                    severity: "error",
                    message: m.to_string(),
                });
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
                report.push(Finding {
                    file: file.clone(),
                    key: u.key.clone(),
                    locale: locale.clone(),
                    code: f.code,
                    severity: match f.severity {
                        text::Severity::Error => "error",
                        text::Severity::Warning => "warning",
                    },
                    message: f.message,
                });
            }
            for (arg, cats) in plurals::icu_cases(t) {
                let missing = plurals::missing(locale, &cats);
                if !missing.is_empty() {
                    report.push(Finding {
                        file: file.clone(),
                        key: u.key.clone(),
                        locale: locale.clone(),
                        code: "plural",
                        severity: "error",
                        message: format!("ICU plural `{arg}` is missing {}", missing.join(", ")),
                    });
                }
            }
        }
    }

    // Format-level plural structures.
    for spec in &cfg.files {
        let file = spec.path.display().to_string();
        let path = root.join(&spec.path);
        match spec.format {
            Format::Xcstrings => {
                let doc = formats::xcstrings::parse(&std::fs::read_to_string(&path)?)?;
                for (key, locale, cats) in plurals::xcstrings_plurals(&doc) {
                    if !locales.contains(&locale) || skip.matches(&spec.path, &key) {
                        continue;
                    }
                    let missing = plurals::missing(&locale, &cats);
                    if !missing.is_empty() {
                        report.push(Finding {
                            file: file.clone(),
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
                let Some(template) = &spec.locale_path else {
                    continue;
                };
                for locale in &locales {
                    let lp = root.join(project::locale_file(template, locale));
                    if !lp.exists() {
                        continue;
                    }
                    let doc = formats::android::parse(&std::fs::read_to_string(&lp)?)
                        .with_context(|| format!("parsing {}", lp.display()))?;
                    for (name, cats) in plurals::android_plurals(&doc) {
                        if skip.matches(&spec.path, &name) {
                            continue;
                        }
                        let missing = plurals::missing(locale, &cats);
                        if !missing.is_empty() {
                            report.push(Finding {
                                file: file.clone(),
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
                let source = formats::json::parse(&std::fs::read_to_string(&path)?)?;
                let source_groups = plurals::i18next_plural_groups(
                    &source.entries.iter().map(|e| e.key()).collect::<Vec<_>>(),
                );
                for locale in &locales {
                    let lp = root.join(project::locale_file(template, locale));
                    if !lp.exists() {
                        continue;
                    }
                    let doc = formats::json::parse(&std::fs::read_to_string(&lp)?)
                        .with_context(|| format!("parsing {}", lp.display()))?;
                    let groups = plurals::i18next_groups(
                        &doc.entries.iter().map(|e| e.key()).collect::<Vec<_>>(),
                    );
                    for base in source_groups.keys() {
                        if skip.matches(&spec.path, base) {
                            continue;
                        }
                        let cats = groups.get(base).cloned().unwrap_or_default();
                        let missing = plurals::missing(locale, &cats);
                        if !missing.is_empty() {
                            report.push(Finding {
                                file: file.clone(),
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
            Format::Arb | Format::Po | Format::Resx => {}
        }
    }
    report.findings.sort_by(|a, b| {
        (&a.file, &a.locale, &a.key, a.code).cmp(&(&b.file, &b.locale, &b.key, b.code))
    });
    Ok(report)
}
