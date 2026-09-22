//! `polygo check signalapp/Signal-iOS`: a repository instead of a path. Cloned shallow
//! into a cache directory (refreshed on the next run) and checked like any directory.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::Command;

/// Does this argument name a repository rather than a file? A URL, an ssh remote, or
/// GitHub's `owner/repo` shorthand when nothing by that name exists locally.
pub fn is_remote(arg: &str, exists_locally: bool) -> bool {
    if ["https://", "http://", "ssh://", "file://", "git@"]
        .iter()
        .any(|p| arg.starts_with(p))
    {
        return true;
    }
    if exists_locally {
        return false;
    }
    // `owner/repo`, but `locales/en.json` that happens not to exist is a typo'd path,
    // not a repository (repo names may still contain dots: `vercel/next.js`).
    const FILE_EXT: &[&str] = &[
        ".json",
        ".xml",
        ".strings",
        ".xcstrings",
        ".stringsdict",
        ".arb",
        ".po",
        ".pot",
        ".resx",
        ".resw",
        ".toml",
        ".yaml",
        ".yml",
        ".txt",
        ".md",
    ];
    let parts: Vec<&str> = arg.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
        && !FILE_EXT
            .iter()
            .any(|e| parts[1].to_ascii_lowercase().ends_with(e))
}

/// The clone URL and a cache path for it: `host/owner/repo`.
fn resolve(arg: &str) -> (String, PathBuf) {
    let url = if arg.contains("://") || arg.starts_with("git@") {
        arg.trim_end_matches('/').to_string()
    } else {
        format!("https://github.com/{}", arg.trim_end_matches('/'))
    };
    let key = url
        .trim_end_matches(".git")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://")
        .trim_start_matches("file://")
        .replace("git@", "")
        .replace(':', "/");
    let mut dir = cache_dir();
    for part in key.split('/').filter(|p| !p.is_empty()) {
        dir.push(part);
    }
    (url, dir)
}

fn cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("POLYGO_CACHE_DIR") {
        return PathBuf::from(d);
    }
    if let Ok(x) = std::env::var("XDG_CACHE_HOME")
        && !x.is_empty()
    {
        return PathBuf::from(x).join("polygo").join("repos");
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".cache")
        .join("polygo")
        .join("repos")
}

fn git(args: &[&str], cwd: Option<&std::path::Path>) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let out = cmd
        .output()
        .context("running git (is it installed and on PATH?)")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!(
            "git {}: {}",
            args[0],
            err.trim().lines().last().unwrap_or("failed")
        );
    }
    Ok(())
}

/// Clone (or refresh) the repository, shallow and single-branch, and return its path.
/// Prints one line on stderr saying what it did.
pub fn fetch(arg: &str, reference: Option<&str>) -> Result<PathBuf> {
    let (url, dir) = resolve(arg);
    let started = std::time::Instant::now();
    if dir.join(".git").exists() {
        let mut args = vec!["fetch", "--depth", "1", "origin"];
        if let Some(r) = reference {
            args.push(r);
        }
        git(&args, Some(&dir))?;
        git(
            &["checkout", "--quiet", "--force", "FETCH_HEAD"],
            Some(&dir),
        )?;
    } else {
        std::fs::create_dir_all(dir.parent().unwrap_or(&dir))?;
        let mut args = vec!["clone", "--quiet", "--depth", "1", "--single-branch"];
        if let Some(r) = reference {
            args.extend(["--branch", r]);
        }
        args.push(&url);
        let target = dir.to_string_lossy().into_owned();
        args.push(&target);
        git(&args, None)?;
    }
    eprintln!(
        "{url}{} → {} ({:.1}s)",
        reference.map(|r| format!(" @{r}")).unwrap_or_default(),
        dir.display(),
        started.elapsed().as_secs_f32()
    );
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_remotes() {
        assert!(is_remote("https://github.com/a/b", false));
        assert!(is_remote("git@github.com:a/b.git", false));
        assert!(is_remote("signalapp/Signal-iOS", false));
        assert!(!is_remote("signalapp/Signal-iOS", true)); // a local dir by that name wins
        assert!(!is_remote("locales/en.json", false));
        assert!(!is_remote("Localizable.xcstrings", false));
        assert!(!is_remote("a/b/c", false));
        assert!(is_remote("vercel/next.js", false));
        assert!(!is_remote("../x", false));
        let (url, dir) = resolve("signalapp/Signal-iOS");
        assert_eq!(url, "https://github.com/signalapp/Signal-iOS");
        assert!(
            dir.ends_with("github.com/signalapp/Signal-iOS"),
            "{}",
            dir.display()
        );
        let (_, dir) = resolve("git@github.com:a/b.git");
        assert!(dir.ends_with("github.com/a/b"), "{}", dir.display());
    }
}
