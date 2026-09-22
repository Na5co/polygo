//! What a check run produces, and the one door every finding goes through: `Cx::emit`
//! applies the skip rules, prefixes the key for multi-file projects and finds the line,
//! so no check can forget any of that.

use crate::check::code::{Code, Severity};
use crate::check::locate::Locator;
use crate::config::{Config, FileSpec, KeySkip};
use crate::project;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    /// The file a fix would be made in: the locale file for per-locale formats, the
    /// catalog for `.xcstrings`.
    pub file: String,
    /// Line of the key in `file`, when it could be found there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// The key, prefixed with the file path when there are several `[[files]]`.
    pub key: String,
    pub locale: String,
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    /// The corrected translation, when the repair is mechanical (a translated placeholder
    /// name put back). `check --fix` writes it without asking a model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
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
    pub fn push(&mut self, f: Finding) {
        match f.severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
        }
        self.findings.push(f);
    }

    /// Keep the findings `keep` says to; recount.
    pub fn retain(&mut self, keep: impl FnMut(&Finding) -> bool) {
        self.findings.retain(keep);
        self.errors = self
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        self.warnings = self.findings.len() - self.errors;
    }

    /// Findings that carry a corrected translation: `(key, locale, text)`, one per
    /// key and locale, in target locales only.
    pub fn fixes(&self, source_locale: &str) -> Vec<(&str, &str, &str)> {
        let mut out: Vec<(&str, &str, &str)> = Vec::new();
        for f in &self.findings {
            if let Some(fix) = &f.fix
                && f.locale != source_locale
                && !out.iter().any(|(k, l, _)| *k == f.key && *l == f.locale)
            {
                out.push((&f.key, &f.locale, fix));
            }
        }
        out
    }

    /// Keys per locale that `--fix` re-translates: errors a new translation can cure,
    /// in target locales only. Findings with a mechanical fix are not among them.
    pub fn fixable_keys(&self, source_locale: &str) -> BTreeMap<String, Vec<String>> {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in &self.findings {
            if f.severity == Severity::Error
                && f.code.fixable()
                && f.fix.is_none()
                && f.locale != source_locale
            {
                let v = m.entry(f.locale.clone()).or_default();
                if !v.contains(&f.key) {
                    v.push(f.key.clone());
                }
            }
        }
        m
    }
}

/// Everything a check needs, and the only way to add a finding.
pub struct Cx<'a> {
    pub root: &'a Path,
    pub cfg: &'a Config,
    /// The locales being checked (`--locale` or every target).
    pub locales: Vec<String>,
    pub report: Report,
    locator: Locator<'a>,
    skip: KeySkip,
    /// `polygo:skip` in a developer comment: (file index, key).
    directive_skip: BTreeSet<(usize, String)>,
}

impl<'a> Cx<'a> {
    pub fn new(
        root: &'a Path,
        cfg: &'a Config,
        locales: Vec<String>,
        directive_skip: BTreeSet<(usize, String)>,
    ) -> anyhow::Result<Self> {
        Ok(Cx {
            root,
            cfg,
            locales,
            report: Report::default(),
            locator: Locator::new(root),
            skip: cfg.key_skip()?,
            directive_skip,
        })
    }

    pub fn spec(&self, idx: usize) -> &'a FileSpec {
        &self.cfg.files[idx]
    }

    /// Absolute path of a spec's source file.
    pub fn source_path(&self, idx: usize) -> std::path::PathBuf {
        self.root.join(&self.cfg.files[idx].path)
    }

    /// Absolute path of a spec's file for `locale`, if the format has one.
    pub fn locale_path(&self, idx: usize, locale: &str) -> Option<std::path::PathBuf> {
        let t = self.cfg.files[idx].locale_path.as_deref()?;
        Some(self.root.join(project::locale_file(t, locale)))
    }

    /// Where a finding for `key` in `locale` lives, for checks that place a finding on a
    /// different key than they report (`photos_other` for the `photos` group).
    pub fn locate(&mut self, idx: usize, key: &str, locale: &str) -> (String, Option<usize>) {
        self.locator.locate(&self.cfg.files[idx], key, locale)
    }

    /// `[keys] skip` or `polygo:skip` for this key (plural / substitution suffix ignored).
    pub fn skipped(&self, idx: usize, key: &str) -> bool {
        let base = key.split('#').next().unwrap_or(key);
        self.skip.matches(&self.cfg.files[idx].path, key)
            || self.directive_skip.contains(&(idx, base.to_string()))
    }

    /// The key as findings and the lockfile name it: prefixed with the file path when the
    /// project has several `[[files]]`.
    pub fn full_key(&self, idx: usize, key: &str) -> String {
        if self.cfg.files.len() > 1 {
            format!("{}:{key}", self.cfg.files[idx].path.display())
        } else {
            key.to_string()
        }
    }

    /// Add a finding for `key` (within file `idx`) in `locale`, unless the key is skipped.
    /// The file and line come from the locator.
    pub fn emit(
        &mut self,
        idx: usize,
        key: &str,
        locale: &str,
        code: Code,
        severity: Severity,
        message: impl Into<String>,
    ) {
        self.emit_fix(idx, key, locale, code, severity, message, None);
    }

    /// `emit` with the corrected translation, when the check knows it.
    #[allow(clippy::too_many_arguments)]
    pub fn emit_fix(
        &mut self,
        idx: usize,
        key: &str,
        locale: &str,
        code: Code,
        severity: Severity,
        message: impl Into<String>,
        fix: Option<String>,
    ) {
        if self.skipped(idx, key) {
            return;
        }
        let (file, line) = self.locator.locate(&self.cfg.files[idx], key, locale);
        let key = self.full_key(idx, key);
        self.report.push(Finding {
            file,
            line,
            key,
            locale: locale.to_string(),
            code,
            severity,
            message: message.into(),
            fix,
        });
    }

    /// Like `emit`, at a file and line the check already knows (a `.stringsdict` entry).
    #[allow(clippy::too_many_arguments)]
    pub fn emit_at(
        &mut self,
        idx: usize,
        file: String,
        line: Option<usize>,
        key: &str,
        locale: &str,
        code: Code,
        severity: Severity,
        message: impl Into<String>,
    ) {
        if self.skipped(idx, key) {
            return;
        }
        let key = self.full_key(idx, key);
        self.report.push(Finding {
            file,
            line,
            key,
            locale: locale.to_string(),
            code,
            severity,
            message: message.into(),
            fix: None,
        });
    }

    /// Emit with the code's default severity.
    pub fn warn_or_err(
        &mut self,
        idx: usize,
        key: &str,
        locale: &str,
        code: Code,
        msg: impl Into<String>,
    ) {
        self.emit(idx, key, locale, code, code.severity(), msg);
    }
}
