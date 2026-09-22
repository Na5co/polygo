//! `polygo-baseline.json`: the findings a project has decided to live with for now, so
//! `check` can fail only on new ones. A finding is identified by (file, key, locale,
//! code): lines move and messages get reworded, the problem does not.
//!
//! `polygo check --write-baseline` records everything current; committed next to
//! polygo.toml it turns a 400-warning legacy catalog into a clean run that still fails on
//! the next regression. `--no-baseline` shows everything again.

use crate::check::run::{Finding, Report};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const FILE_NAME: &str = "polygo-baseline.json";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Entry {
    pub file: String,
    pub key: String,
    pub locale: String,
    pub code: String,
}

impl Entry {
    fn of(f: &Finding) -> Entry {
        Entry {
            file: f.file.clone(),
            key: f.key.clone(),
            locale: f.locale.clone(),
            code: f.code.to_string(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Baseline {
    pub version: u32,
    /// What produced it, for humans reading the diff.
    #[serde(default)]
    pub note: String,
    pub findings: Vec<Entry>,
}

/// What applying a baseline did to a report.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Applied {
    /// Findings hidden because the baseline lists them.
    pub known: usize,
    /// Baseline entries that matched nothing: fixed since, or renamed.
    pub stale: usize,
}

pub fn load(root: &Path) -> Result<Option<Baseline>> {
    let path = root.join(FILE_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(
        serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))?,
    ))
}

pub fn write(root: &Path, report: &Report) -> Result<usize> {
    let mut findings: Vec<Entry> = report.findings.iter().map(Entry::of).collect();
    findings.sort();
    findings.dedup();
    let n = findings.len();
    let b = Baseline {
        version: 1,
        note: format!(
            "polygo check --write-baseline · {} finding(s) accepted as known; delete an entry to make check report it again",
            n
        ),
        findings,
    };
    std::fs::write(
        root.join(FILE_NAME),
        format!("{}\n", serde_json::to_string_pretty(&b)?),
    )?;
    Ok(n)
}

/// Remove known findings from `report` (recounting errors and warnings) and say how
/// many were hidden and how many baseline entries no longer match anything.
pub fn apply(report: &mut Report, baseline: &Baseline) -> Applied {
    let known: BTreeSet<&Entry> = baseline.findings.iter().collect();
    let mut seen: BTreeSet<Entry> = BTreeSet::new();
    let before = report.findings.len();
    let kept: Vec<Finding> = report
        .findings
        .drain(..)
        .filter(|f| {
            let e = Entry::of(f);
            if known.contains(&e) {
                seen.insert(e);
                false
            } else {
                true
            }
        })
        .collect();
    let hidden = before - kept.len();
    report.findings = kept;
    report.errors = report
        .findings
        .iter()
        .filter(|f| f.severity == crate::check::code::Severity::Error)
        .count();
    report.warnings = report.findings.len() - report.errors;
    Applied {
        known: hidden,
        stale: known.len() - seen.len(),
    }
}
