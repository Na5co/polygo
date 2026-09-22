//! G5.5: the Action's `review: true` script, against a stub `gh`: it reads the pull
//! request's diff, posts one review, and says nothing it has already said.
#![cfg(unix)] // runs the shell script; the Action itself only runs on ubuntu
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// A `gh` that answers from files in `dir` and records what would be posted.
fn stub_gh(dir: &Path) {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        r#"#!/usr/bin/env bash
# Stub: `api .../files` prints the diff, `api .../comments` the existing comments,
# and a POST is written to posted.json.
for a in "$@"; do case "$a" in *"/files") exec cat "$STUB_DIR/files.diff";; *"/comments") exec cat "$STUB_DIR/comments.json";; esac; done
if [ "${1:-}" = "api" ] && [ "${2:-}" = "--method" ]; then cat > "$STUB_DIR/posted.json"; echo '{}'; exit 0; fi
exit 1
"#,
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"greet\": \"Hi {{ name }}\",\n  \"left\": \"{{count}} left\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{ nome }}\",\n  \"left\": \"{{anzahl}} übrig\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("files.diff"),
        "--- a/locales/de.json\n+++ b/locales/de.json\n@@ -2,2 +2,2 @@\n-  \"greet\": \"Hallo\",\n-  \"left\": \"einige übrig\",\n+  \"greet\": \"Hallo {{ nome }}\",\n+  \"left\": \"{{anzahl}} übrig\",\n",
    )
    .unwrap();
    fs::write(
        root.join("event.json"),
        "{\"pull_request\": {\"number\": 7, \"base\": {\"sha\": \"deadbeef\"}}}",
    )
    .unwrap();
}

fn run(root: &Path) -> (bool, String) {
    let out = Command::new("bash")
        .current_dir(root)
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("action/review.sh")
                .to_str()
                .unwrap(),
        )
        .env(
            "PATH",
            format!(
                "{}:{}",
                root.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("STUB_DIR", root)
        .env("POLYGO_BIN", env!("CARGO_BIN_EXE_polygo"))
        .env("POLYGO_PATH", "locales")
        .env("POLYGO_CONFIG_DIR", root.join(".polygo-config"))
        .env("GITHUB_EVENT_PATH", root.join("event.json"))
        .env("GITHUB_REPOSITORY", "acme/app")
        .env("GH_TOKEN", "x")
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

#[test]
fn action_review_posts_once_and_then_stays_quiet() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stub_gh(root);
    fs::write(root.join("comments.json"), "[]\n").unwrap();

    let (ok, log) = run(root);
    assert!(ok, "{log}");
    assert!(log.contains("polygo review: 2 comment(s) posted"), "{log}");
    let posted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("posted.json")).unwrap()).unwrap();
    assert_eq!(posted["event"], "COMMENT");
    let comments = posted["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["path"], "locales/de.json");
    assert!(
        comments[0]["body"]
            .as_str()
            .unwrap()
            .contains("```suggestion\n  \"greet\": \"Hallo {{ name }}\",\n```"),
        "{}",
        comments[0]["body"]
    );

    // A second push: polygo has already said both things, so nothing is posted again.
    fs::write(
        root.join("comments.json"),
        "[\"locales/de.json:2\",\"locales/de.json:3\"]\n",
    )
    .unwrap();
    fs::remove_file(root.join("posted.json")).unwrap();
    let (ok, log) = run(root);
    assert!(ok, "{log}");
    assert!(log.contains("nothing new to say"), "{log}");
    assert!(!root.join("posted.json").exists());
}

#[test]
fn action_review_skips_anything_that_is_not_a_pull_request() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stub_gh(root);
    fs::write(root.join("event.json"), "{\"ref\": \"refs/heads/main\"}").unwrap();
    let (ok, log) = run(root);
    assert!(ok, "{log}");
    assert!(log.contains("not a pull_request event"), "{log}");
}
