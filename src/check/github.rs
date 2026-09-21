//! `polygo check --github`: workflow commands GitHub Actions turns into annotations on the
//! file in a pull request's Files tab, plus a table in the job summary.

use crate::check::run::Report;
use anyhow::Result;
use std::io::Write;

/// Escape for the message part of a workflow command (`::error ...::message`).
fn msg(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escape for a property value (`file=...,title=...`).
fn prop(s: &str) -> String {
    msg(s).replace(':', "%3A").replace(',', "%2C")
}

pub fn print(report: &Report, strict: bool) -> Result<()> {
    // Under --strict a warning fails the job, so it is an error everywhere here: the
    // annotation, the totals line and the summary must agree with the exit code.
    let report = if strict {
        let mut r = report.clone();
        for f in &mut r.findings {
            f.severity = "error";
        }
        r.errors += r.warnings;
        r.warnings = 0;
        r
    } else {
        report.clone()
    };
    let report = &report;
    let out = std::io::stdout();
    let mut out = out.lock();
    for f in &report.findings {
        let sev = f.severity;
        let key: String = f.key.chars().take(80).collect();
        let line = f.line.map(|l| format!(",line={l}")).unwrap_or_default();
        writeln!(
            out,
            "::{sev} file={}{line},title={}::{}",
            prop(&f.file),
            prop(&format!("polygo {} [{}]", f.code, f.locale)),
            msg(&format!("{key}: {}", f.message))
        )?;
    }
    writeln!(
        out,
        "polygo check: {} error(s), {} warning(s), {} translation(s) checked",
        report.errors, report.warnings, report.checked
    )?;
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY")
        && !path.is_empty()
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        f.write_all(summary(report).as_bytes())?;
    }
    Ok(())
}

/// Markdown for the job summary: totals, then the first 100 findings.
pub fn summary(report: &Report) -> String {
    let mut s = String::from("## polygo check\n\n");
    if report.findings.is_empty() {
        s.push_str(&format!(
            "✅ {} translation(s) checked, no problems.\n",
            report.checked
        ));
    } else {
        s.push_str(&format!(
            "**{} error(s), {} warning(s)** in {} translation(s).\n\n| | file | key | locale | problem |\n|---|---|---|---|---|\n",
            report.errors, report.warnings, report.checked
        ));
        let cell = |t: &str| t.replace('|', "\\|").replace('\n', "⏎");
        for f in report.findings.iter().take(100) {
            let key: String = f.key.chars().take(60).collect();
            s.push_str(&format!(
                "| {} | `{}` | `{}` | {} | {}: {} |\n",
                if f.severity == "error" {
                    "❌"
                } else {
                    "⚠️"
                },
                match f.line {
                    Some(l) => format!("{}:{l}", cell(&f.file)),
                    None => cell(&f.file),
                },
                cell(&key),
                f.locale,
                f.code,
                cell(&f.message)
            ));
        }
        if report.findings.len() > 100 {
            s.push_str(&format!(
                "\n…and {} more. `polygo check` locally prints them all.\n",
                report.findings.len() - 100
            ));
        }
    }
    let mut gaps: Vec<(&String, usize, usize)> = report
        .coverage
        .iter()
        .filter(|(_, (d, t))| d < t)
        .map(|(l, (d, t))| (l, *d, *t))
        .collect();
    if !gaps.is_empty() {
        gaps.sort_by_key(|(l, d, t)| (d * 100 / (*t).max(1), (*l).clone()));
        let shown: Vec<String> = gaps
            .iter()
            .take(10)
            .map(|(l, d, t)| format!("`{l}` {}% ({} missing)", d * 100 / (*t).max(1), t - d))
            .collect();
        let more = gaps.len().saturating_sub(10);
        s.push_str(&format!(
            "\nCoverage: {}{}.\n",
            shown.join(", "),
            if more > 0 {
                format!(", {more} more")
            } else {
                String::new()
            }
        ));
    }
    s.push_str("\n<sub>[polygo](https://github.com/Na5co/polygo) · runs in milliseconds, no model needed.</sub>\n");
    s
}
