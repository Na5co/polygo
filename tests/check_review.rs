//! G3.6: `polygo check --review` is a pull-request review GitHub accepts: comments only on
//! lines the diff touches, and a committable suggestion for every mechanical fix.
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

fn polygo(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(cwd)
        .arg("check")
        .args(args)
        .env("POLYGO_CONFIG_DIR", cwd.join(".polygo-config"))
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A project where `greet` and `left` have a translated placeholder name and `bye` is
/// still the English text; only the first two lines are in the diff.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"greet\": \"Hi {{ name }}\",\n  \"left\": \"{{count}} left\",\n  \"bye\": \"Delete everything now\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{ nome }}\",\n  \"left\": \"{{anzahl}} übrig\",\n  \"bye\": \"Delete everything now\"\n}\n",
    )
    .unwrap();
}

const DIFF: &str = "--- a/locales/de.json\n+++ b/locales/de.json\n@@ -2,2 +2,2 @@\n-  \"greet\": \"Hallo\",\n-  \"left\": \"einige übrig\",\n+  \"greet\": \"Hallo {{ nome }}\",\n+  \"left\": \"{{anzahl}} übrig\",\n";

#[test]
fn review_comments_on_the_diff_with_committable_suggestions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(root.join("pr.diff"), DIFF).unwrap();
    let (code, out, _) = polygo(root, &["locales", "--review", "--diff", "pr.diff"]);
    assert_eq!(code, 1, "errors still fail the job");
    let v: Value = serde_json::from_str(&out).unwrap();
    // A review, not an approval or a block: the exit code fails the job, the bot only talks.
    assert_eq!(v["event"], "COMMENT");
    let comments = v["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "{out}");
    assert_eq!(comments[0]["path"], "locales/de.json");
    assert_eq!(comments[0]["line"], 2);
    assert_eq!(comments[0]["side"], "RIGHT");
    let body = comments[0]["body"].as_str().unwrap();
    assert!(body.starts_with("<!-- polygo -->"), "{body}");
    assert!(body.contains("placeholder name translated: {{name}} → {{nome}}"));
    // The suggestion is the whole line as the format's own writer would write it, so
    // "Commit suggestion" leaves a valid file behind.
    assert!(
        body.contains("```suggestion\n  \"greet\": \"Hallo {{ name }}\",\n```"),
        "{body}"
    );
    assert!(
        comments[1]["body"]
            .as_str()
            .unwrap()
            .contains("```suggestion\n  \"left\": \"{{count}} übrig\",\n```"),
        "{}",
        comments[1]["body"]
    );
    // The summary counts the suggestions and says what is outside the diff (`bye`).
    let summary = v["body"].as_str().unwrap();
    assert!(
        summary.contains("2 of them have a suggested change"),
        "{summary}"
    );
    assert!(
        summary.contains("1 finding(s) are outside this diff"),
        "{summary}"
    );
}

#[test]
fn review_without_a_diff_covers_everything_and_reads_git() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    // No diff: every finding gets a comment (a local preview).
    let (_, out, _) = polygo(root, &["locales", "--review"]);
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["comments"].as_array().unwrap().len(), 3, "{out}");
    assert!(!v["body"].as_str().unwrap().contains("outside this diff"));

    // --base asks git for the same thing the diff said.
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(root)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo\",\n  \"left\": \"einige übrig\",\n  \"bye\": \"Delete everything now\"\n}\n",
    )
    .unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "base"]);
    fixture(root);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "translate"]);
    let (_, out, err) = polygo(root, &["locales", "--review", "--base", "HEAD~1"]);
    let v: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    let comments = v["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "{out}");
    assert_eq!(comments[0]["line"], 2);
    assert_eq!(comments[1]["line"], 3);
}

#[test]
fn review_says_nothing_is_wrong_when_nothing_is() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"greet\": \"Hi {{ name }}\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{ name }}\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = polygo(root, &["locales", "--review"]);
    assert_eq!(code, 0);
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v["comments"].as_array().unwrap().is_empty());
    assert!(
        v["body"]
            .as_str()
            .unwrap()
            .contains("1 translation(s) checked, no problems"),
        "{out}"
    );
}
