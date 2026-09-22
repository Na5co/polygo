//! `polygo check --review`: the findings as a GitHub pull-request review, ready to POST to
//! `/repos/{owner}/{repo}/pulls/{n}/reviews`. One inline comment per file and line, and for
//! a finding polygo can repair (a translated placeholder name) a ```suggestion block the
//! maintainer commits with one click.
//!
//! GitHub rejects a whole review when any comment sits on a line the diff does not touch,
//! so `--base <ref>` restricts the comments to the lines this branch changed.

use crate::check::code::Severity;
use crate::check::run::Report;
use crate::config::Config;
use crate::project;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// At most this many inline comments in one review: GitHub is slow past a few dozen, and a
/// wall of comments is noise. The rest are counted in the summary.
const MAX_COMMENTS: usize = 40;

pub struct Options<'a> {
    /// Restrict comments to lines changed since this ref (`git diff base...HEAD`).
    pub base: Option<&'a str>,
    /// Restrict comments to the lines a unified diff touches, instead of asking git: what
    /// `GET /pulls/{n}/files` returns, for a reviewer that has no checkout.
    pub diff: Option<&'a str>,
    /// Prefix `Report::file` carries but the tree does not (a `polygo check <path>` run).
    pub prefix: Option<&'a str>,
    /// Warnings are errors, as everywhere else under `--strict`.
    pub strict: bool,
}

pub fn render(root: &Path, cfg: &Config, report: &Report, opts: &Options) -> Result<Value> {
    let changed = match (opts.base, opts.diff) {
        // A diff from GitHub names files from the repository root, as the findings do.
        (_, Some(text)) => Some(parse_diff(text, None)),
        (Some(base), None) => Some(changed_lines(root, base, opts.prefix)?),
        (None, None) => None,
    };
    let suggestions = suggestions(root, cfg, report, opts.prefix).unwrap_or_default();

    // One comment per file and line: several findings on one string read as one remark.
    let mut groups: BTreeMap<(&str, usize), Vec<&crate::check::report::Finding>> = BTreeMap::new();
    let mut unplaced = 0;
    for f in &report.findings {
        let Some(line) = f.line else {
            unplaced += 1;
            continue;
        };
        if let Some(c) = &changed
            && !c.get(f.file.as_str()).is_some_and(|ls| ls.contains(&line))
        {
            unplaced += 1;
            continue;
        }
        groups.entry((&f.file, line)).or_default().push(f);
    }
    let shown = groups.len().min(MAX_COMMENTS);
    let comments: Vec<Value> = groups
        .iter()
        .take(MAX_COMMENTS)
        .map(|((file, line), fs)| {
            json!({
                "path": file,
                "line": line,
                "side": "RIGHT",
                "body": comment(fs, suggestions.get(&(file.to_string(), *line)), opts.strict),
            })
        })
        .collect();
    let hidden = groups.len() - shown;
    Ok(json!({
        "event": "COMMENT",
        "body": body(report, &suggestions, hidden, unplaced, opts.strict),
        "comments": comments,
    }))
}

/// The body of one inline comment: what is wrong, and the repair when there is one.
fn comment(
    findings: &[&crate::check::report::Finding],
    suggestion: Option<&String>,
    strict: bool,
) -> String {
    let mut s = String::from("<!-- polygo -->\n");
    for f in findings {
        let icon = if f.severity == Severity::Error || strict {
            "❌"
        } else {
            "⚠️"
        };
        let key: String = f
            .key
            .chars()
            .take(80)
            .collect::<String>()
            .replace('\n', "⏎");
        s.push_str(&format!(
            "{icon} **{}** · `{}` [{}]\n\n{}\n\n",
            f.code.title(),
            key.replace('`', "'"),
            f.locale,
            f.message
        ));
    }
    if let Some(fixed) = suggestion {
        // A line holding a fence of its own would end the block early.
        let fence = if fixed.contains("```") { "````" } else { "```" };
        s.push_str(&format!("{fence}suggestion\n{fixed}\n{fence}\n\n"));
    }
    s.push_str("<sub>[polygo](https://github.com/Na5co/polygo)</sub>");
    s
}

/// The review's own message: the totals, what it could not place, and the one-liner that
/// repairs everything mechanical at once.
fn body(
    report: &Report,
    suggestions: &BTreeMap<(String, usize), String>,
    hidden: usize,
    unplaced: usize,
    strict: bool,
) -> String {
    let (errors, warnings) = if strict {
        (report.errors + report.warnings, 0)
    } else {
        (report.errors, report.warnings)
    };
    let mut s = String::from("## polygo check\n\n");
    if report.findings.is_empty() {
        s.push_str(&format!(
            "✅ {} translation(s) checked, no problems.\n",
            report.checked
        ));
    } else {
        s.push_str(&format!(
            "**{errors} error(s), {warnings} warning(s)** in {} translation(s) checked.\n",
            report.checked
        ));
    }
    if !suggestions.is_empty() {
        s.push_str(&format!(
            "\n{} of them {} a suggested change you can commit from this review — or run `polygo check --fix` to apply every one at once (no model involved).\n",
            suggestions.len(),
            if suggestions.len() == 1 { "has" } else { "have" }
        ));
    }
    if hidden > 0 {
        s.push_str(&format!(
            "\n{hidden} more finding(s) on changed lines are not commented here (a review holds {MAX_COMMENTS}).\n"
        ));
    }
    if unplaced > 0 {
        s.push_str(&format!(
            "\n{unplaced} finding(s) are outside this diff — existing translations this pull request does not touch. `polygo check` locally, or the job summary, lists them.\n"
        ));
    }
    s.push_str("\n<sub>[polygo](https://github.com/Na5co/polygo) · milliseconds, no model, no code leaves the runner.</sub>");
    s
}

/// Lines each file gained or changed in `base...HEAD`, keyed as the findings name them.
fn changed_lines(
    root: &Path,
    base: &str,
    prefix: Option<&str>,
) -> Result<BTreeMap<String, BTreeSet<usize>>> {
    let out = std::process::Command::new("git")
        .current_dir(root)
        // `--relative`: paths as the findings have them, relative to the checked root
        // rather than to the repository.
        .args([
            "diff",
            "--unified=0",
            "--no-color",
            "--relative",
            &format!("{base}...HEAD"),
        ])
        .output()
        .context("running git diff (is this a git checkout?)")?;
    if !out.status.success() {
        anyhow::bail!(
            "git diff {base}...HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(parse_diff(&String::from_utf8_lossy(&out.stdout), prefix))
}

/// The `+++ b/<path>` and `@@` headers of a unified diff: which lines each file holds on
/// the new side, which are the only lines GitHub accepts a comment on.
fn parse_diff(text: &str, prefix: Option<&str>) -> BTreeMap<String, BTreeSet<usize>> {
    let mut map: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut file = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            file = match prefix {
                Some(p) => format!("{p}/{rest}"),
                None => rest.to_string(),
            };
        } else if let Some(rest) = line.strip_prefix("@@ ")
            && !file.is_empty()
        {
            // `@@ -12,3 +14,2 @@`: the lines this side of the diff holds.
            if let Some(plus) = rest.split('+').nth(1) {
                let spec = plus.split(' ').next().unwrap_or("");
                let (start, count) = match spec.split_once(',') {
                    Some((a, b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(0)),
                    None => (spec.parse().unwrap_or(0), 1usize),
                };
                let e = map.entry(file.clone()).or_default();
                for l in start..start + count {
                    e.insert(l);
                }
            }
        }
    }
    map
}

/// The corrected line for every finding polygo can repair: the fixes are applied to a copy
/// of the string files and the result is diffed line by line, so the suggestion is what the
/// format's own writer would produce, escaping and indentation included.
fn suggestions(
    root: &Path,
    cfg: &Config,
    report: &Report,
    prefix: Option<&str>,
) -> Result<BTreeMap<(String, usize), String>> {
    let fixes = report.fixes(&cfg.source_locale);
    if fixes.is_empty() {
        return Ok(BTreeMap::new());
    }
    let strip = |file: &str| match prefix {
        Some(p) => file
            .strip_prefix(p)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(file)
            .to_string(),
        None => file.to_string(),
    };
    let tmp = Scratch::new()?;
    let (tmp, mut copied) = (tmp.0.as_path(), Vec::new());
    for rel in touched_files(cfg) {
        let from = root.join(&rel);
        if !from.is_file() {
            continue;
        }
        let to = tmp.join(&rel);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(&from, &to)?;
        copied.push(rel);
    }
    crate::engine::write_translations(tmp, cfg, &fixes)?;

    // Which lines the writer changed, per file.
    let mut lines: BTreeMap<String, BTreeMap<usize, String>> = BTreeMap::new();
    for rel in copied {
        let (before, after) = (
            crate::formats::read_text(&root.join(&rel)),
            crate::formats::read_text(&tmp.join(&rel)),
        );
        let (Ok(before), Ok(after)) = (before, after) else {
            continue;
        };
        let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
        // A writer that reflowed the file has no line-for-line answer to offer.
        if a.len() != b.len() {
            continue;
        }
        let key = rel.to_string_lossy().replace('\\', "/");
        let e = lines.entry(key).or_default();
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            if x != y {
                e.insert(i + 1, (*y).to_string());
            }
        }
    }
    let mut out = BTreeMap::new();
    for f in &report.findings {
        if f.fix.is_none() {
            continue;
        }
        if let Some(line) = f.line
            && let Some(text) = lines.get(&strip(&f.file)).and_then(|m| m.get(&line))
        {
            out.insert((f.file.clone(), line), text.clone());
        }
    }
    Ok(out)
}

/// A directory of its own for the copy the fixes are applied to, removed when the check
/// is done with it.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir =
            std::env::temp_dir().join(format!("polygo-review-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every string file the project has, relative to the root: sources, and one per locale.
fn touched_files(cfg: &Config) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for spec in &cfg.files {
        out.push(spec.path.clone());
        if let Some(t) = &spec.locale_path {
            for locale in std::iter::once(&cfg.source_locale).chain(&cfg.target_locales) {
                out.push(PathBuf::from(project::locale_file(t, locale)));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}
