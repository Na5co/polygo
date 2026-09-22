//! G3.7: a string file that does not parse is reported, not quietly left out of the run.
use std::fs;
use std::path::Path;
use std::process::Command;

fn check(cwd: &Path, args: &[&str]) -> (i32, String, String) {
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

fn project(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"a\": \"Hi {{name}}\",\n  \"b\": \"Goodbye for now\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"a\": \"Hallo {{name}}\",\n  \"b\": \"Tschüss erstmal\"\n}\n",
    )
    .unwrap();
}

#[test]
fn a_broken_locale_file_is_an_error_not_a_missing_locale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project(root);
    // A comma dropped: the file no longer parses. Without the syntax check the detector
    // cannot read it, the locale vanishes and the run says everything is fine.
    fs::write(
        root.join("locales/fr.json"),
        "{\n  \"a\": \"Bonjour {{name}}\"\n  \"b\": \"Au revoir\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(root, &["locales"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("locales/fr.json:3"), "{out}");
    assert!(out.contains("[fr]  syntax: cannot be parsed:"), "{out}");
    // The locales that do parse are still checked, so one broken file does not hide the
    // rest of the project.
    let (_, out, _) = check(root, &["locales", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["coverage"]["de"].is_array(), "{out}");
    let f = &v["findings"][0];
    assert_eq!(f["code"], "syntax");
    assert_eq!(f["severity"], "error");
    assert_eq!(f["locale"], "fr");
    assert_eq!(f["line"], 3);
}

#[test]
fn a_broken_file_the_project_needs_stops_the_check_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project(root);
    fs::write(
        root.join("locales/fr.json"),
        "{\n  \"a\": \"Bonjour {{name}}\"\n  \"b\": \"Au revoir\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"fr\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    // fr is a configured target, so the run cannot go on without it — but it says why,
    // with the file and the line, instead of failing with a stack of context.
    let (code, out, err) = check(root, &[]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(out.contains("locales/fr.json:3"), "{out}");
    assert!(out.contains("syntax: cannot be parsed"), "{out}");
    assert!(out.contains("1 error(s)"), "{out}");
}

#[test]
fn a_broken_android_resource_file_too() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("res/values")).unwrap();
    fs::create_dir_all(root.join("res/values-de")).unwrap();
    fs::write(
        root.join("res/values/strings.xml"),
        "<resources><string name=\"a\">Hi %s</string></resources>\n",
    )
    .unwrap();
    fs::write(
        root.join("res/values-de/strings.xml"),
        "<resources><string name=\"a\">Hallo %s</resources>\n",
    )
    .unwrap();
    let (code, out, _) = check(root, &["res"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("values-de/strings.xml"), "{out}");
    assert!(
        out.contains("syntax: cannot be parsed: XML syntax error"),
        "{out}"
    );
}

#[test]
fn a_project_where_everything_parses_says_nothing_about_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project(root);
    let (code, out, _) = check(root, &["locales"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("syntax"), "{out}");
}

#[test]
fn the_reviewer_comments_on_the_broken_line() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project(root);
    fs::write(
        root.join("locales/fr.json"),
        "{\n  \"a\": \"Bonjour {{name}}\"\n  \"b\": \"Au revoir\"\n}\n",
    )
    .unwrap();
    let diff = "--- a/locales/fr.json\n+++ b/locales/fr.json\n@@ -1,4 +1,4 @@\n+{\n+  \"a\": \"Bonjour {{name}}\"\n+  \"b\": \"Au revoir\"\n+}\n";
    fs::write(root.join("pr.diff"), diff).unwrap();
    let (_, out, _) = check(root, &["locales", "--review", "--diff", "pr.diff"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let c = &v["comments"][0];
    assert_eq!(c["path"], "locales/fr.json");
    assert_eq!(c["line"], 3);
    assert!(
        c["body"].as_str().unwrap().contains("Unparsable file"),
        "{}",
        c["body"]
    );
}
