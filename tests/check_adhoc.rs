//! `polygo check` without polygo.toml: point it at a file or a directory and it detects
//! the format, the locales and runs every validator. The 30-second "does my existing
//! localization have bugs?" path: no model, no config, no translating.
use std::fs;
use std::path::Path;
use std::process::Command;

fn corpus(rel: &str) -> String {
    format!("{}/tests/corpus/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn check(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(cwd)
        .arg("check")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn check_a_single_xcstrings_file_with_no_config() {
    let dir = tempfile::tempdir().unwrap();
    fs::copy(
        corpus("xcstrings/icecubes.xcstrings"),
        dir.path().join("Localizable.xcstrings"),
    )
    .unwrap();
    // Absolute path from anywhere.
    let (code, out, err) = check(
        Path::new("/"),
        &[dir.path().join("Localizable.xcstrings").to_str().unwrap()],
    );
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        err.contains("no polygo.toml: checking Localizable.xcstrings"),
        "{err}"
    );
    assert!(out.contains("plural") && out.contains("error(s)"), "{out}");
    // Relative path from the directory.
    let (code, out, _) = check(dir.path(), &["Localizable.xcstrings", "--json"]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["errors"].as_u64().unwrap() > 0);
    assert_eq!(v["findings"][0]["file"], "Localizable.xcstrings");
}

#[test]
fn check_an_android_res_tree_by_file_or_directory() {
    let dir = tempfile::tempdir().unwrap();
    let res = dir.path().join("app/src/main/res");
    fs::create_dir_all(res.join("values")).unwrap();
    fs::create_dir_all(res.join("values-pl")).unwrap();
    fs::write(
        res.join("values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"hello\">Hello %1$s</string>\n    <plurals name=\"n\">\n        <item quantity=\"one\">%d file</item>\n        <item quantity=\"other\">%d files</item>\n    </plurals>\n</resources>\n",
    )
    .unwrap();
    // Polish: placeholder dropped, and only two plural forms.
    fs::write(
        res.join("values-pl/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"hello\">Cześć</string>\n    <plurals name=\"n\">\n        <item quantity=\"one\">%d plik</item>\n        <item quantity=\"other\">%d plików</item>\n    </plurals>\n</resources>\n",
    )
    .unwrap();
    // The source file: polygo has to climb to `res/` to see the layout.
    let (code, out, err) = check(dir.path(), &["app/src/main/res/values/strings.xml"]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(out.contains("placeholders: missing %1$s"), "{out}");
    assert!(out.contains("missing plural form(s) few, many"), "{out}");
    assert!(out.contains("2 error(s)"), "{out}");
    // The whole project directory, from outside it.
    let (code, out, _) = check(Path::new("/"), &[dir.path().to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(out.contains("2 error(s)"), "{out}");
    // And plain `polygo check` inside a project with no polygo.toml.
    let (code, out, err) = check(dir.path(), &[]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        err.contains("no polygo.toml: checking 1 detected file(s)"),
        "{err}"
    );
    assert!(err.contains("polygo init"), "{err}");
    assert!(out.contains("2 error(s)"), "{out}");
}

#[test]
fn check_i18next_locales_dir_and_the_unhelpful_cases() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(
        dir.path().join("locales/en.json"),
        "{\n  \"greet\": \"Hi {{name}}\"\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{nome}}\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(dir.path(), &["locales/en.json"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("[de]") && out.contains("placeholders"),
        "{out}"
    );
    let (code, out, _) = check(dir.path(), &["locales"]);
    assert_eq!(code, 1, "{out}");
    // Clean project: exit 0 and says what it checked.
    fs::write(
        dir.path().join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{name}}\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(dir.path(), &["locales"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("check: ok (1 translation(s) in 1 locale(s))"),
        "{out}"
    );
    // A file polygo cannot place is a clear error listing what it understands.
    fs::write(dir.path().join("notes.txt"), "hi").unwrap();
    let (code, _, err) = check(dir.path(), &["notes.txt"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("notes.txt") && err.contains(".xcstrings"),
        "{err}"
    );
    // No config and nothing detected: the same guidance `init` gives.
    let empty = tempfile::tempdir().unwrap();
    let (code, _, err) = check(empty.path(), &[]);
    assert_eq!(code, 1);
    assert!(
        err.contains("no polygo.toml") && err.contains("polygo init"),
        "{err}"
    );
    // --fix needs a real project (it calls a model).
    let (code, _, err) = check(dir.path(), &["locales", "--fix"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("--fix") && err.contains("polygo init"),
        "{err}"
    );
}
