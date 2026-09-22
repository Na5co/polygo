//! One pull request, start to finish: fetch it, check it, leave the review. The check and
//! the review are polygo's own — the bot adds no rules of its own, so what it says on a
//! pull request is what `polygo check` says on a laptop.

use crate::github::{self, App};
use crate::webhook::Event;
use anyhow::{Context, Result};
use polygo::config::Config;
use serde_json::Value;
use std::path::Path;

/// What happened, for the log.
pub fn review(app: &App, ev: &Event) -> Result<String> {
    let token = app.installation_token(ev.installation)?;
    let diff = github::diff(&ev.repo, ev.number, &token)?;
    if !touches_strings(&diff) {
        return Ok(format!(
            "{}#{}: no string files in the diff",
            ev.repo, ev.number
        ));
    }
    let dir = Scratch::new(&format!("{}-{}", ev.repo.replace('/', "-"), ev.number))?;
    github::fetch_head(&ev.repo, ev.number, &token, dir.path())?;

    let (cfg, root) = project(dir.path())?;
    let opts = polygo::check::run::Options {
        locales: None,
        length_ratio: cfg.length_ratio,
    };
    let report = polygo::check::run::run(&root, &cfg, &opts)?;
    // Findings name files relative to the checked root; the review names them relative to
    // the repository, which is the same thing unless the project sits in a subdirectory.
    let prefix = root
        .strip_prefix(dir.path())
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|p| !p.is_empty());
    let mut review = polygo::check::review::render(
        &root,
        &cfg,
        &report,
        &polygo::check::review::Options {
            base: None,
            diff: Some(&diff),
            prefix: prefix.as_deref(),
            strict: false,
        },
    )?;
    let said = github::said_already(&ev.repo, ev.number, &token)?;
    let left = drop_repeats(&mut review, &said);
    if left == 0 {
        return Ok(format!(
            "{}#{}: {} finding(s), nothing new to say",
            ev.repo,
            ev.number,
            report.findings.len()
        ));
    }
    github::post_review(&ev.repo, ev.number, &token, &review)?;
    Ok(format!(
        "{}#{}: {left} comment(s) posted ({} error(s), {} warning(s) in {} translation(s))",
        ev.repo, ev.number, report.errors, report.warnings, report.checked
    ))
}

/// The project as polygo sees it: `polygo.toml` when the repository has one, otherwise
/// whatever the tree itself says.
fn project(dir: &Path) -> Result<(Config, std::path::PathBuf)> {
    if dir.join(polygo::config::FILE_NAME).exists() {
        return Ok((Config::load(dir)?, dir.to_path_buf()));
    }
    polygo::init::detect_for_check(dir).context("no polygo.toml, and no string files detected")
}

/// Is any file in the diff one polygo would read? A pull request that touches no strings
/// is dropped before the repository is fetched at all.
fn touches_strings(diff: &str) -> bool {
    diff.lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .any(|f| {
            let f = f.to_ascii_lowercase();
            f.ends_with(".xcstrings")
                || f.ends_with(".strings")
                || f.ends_with(".stringsdict")
                || f.ends_with(".arb")
                || f.ends_with(".po")
                || f.ends_with(".resx")
                || f.ends_with("strings.xml")
                || (f.ends_with(".json")
                    && (f.contains("locale")
                        || f.contains("lang")
                        || f.contains("i18n")
                        || f.contains("translation")))
        })
}

/// Drop the comments the bot has already left on the same file and line; returns how many
/// remain to say.
fn drop_repeats(review: &mut Value, said: &[String]) -> usize {
    let Some(comments) = review["comments"].as_array_mut() else {
        return 0;
    };
    comments.retain(|c| {
        let (Some(path), Some(line)) = (c["path"].as_str(), c["line"].as_u64()) else {
            return false;
        };
        !said.contains(&format!("{path}:{line}"))
    });
    comments.len()
}

/// A directory per pull request, removed when the review is done with it.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Result<Scratch> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let dir = std::env::temp_dir().join(format!("polygo-bot-{safe}-{stamp}"));
        std::fs::create_dir_all(&dir)?;
        Ok(Scratch(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_diffs_with_string_files_are_worth_fetching() {
        assert!(touches_strings("+++ b/app/Localizable.xcstrings\n"));
        assert!(touches_strings("+++ b/res/values-de/strings.xml\n"));
        assert!(touches_strings(
            "+++ b/src/lib/i18n/locales/de/translation.json\n"
        ));
        assert!(touches_strings("+++ b/locale/django.po\n"));
        assert!(!touches_strings("+++ b/src/main.rs\n+++ b/package.json\n"));
        assert!(!touches_strings(""));
    }

    #[test]
    fn a_comment_is_made_once() {
        let mut review = json!({
            "comments": [
                { "path": "a.json", "line": 2, "body": "x" },
                { "path": "a.json", "line": 9, "body": "y" },
            ]
        });
        assert_eq!(drop_repeats(&mut review, &["a.json:2".to_string()]), 1);
        assert_eq!(review["comments"][0]["line"], 9);
        assert_eq!(drop_repeats(&mut review, &["a.json:9".to_string()]), 0);
    }
}
