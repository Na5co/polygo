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

#[test]
fn a_locale_file_nobody_listed_is_not_silently_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project(root);
    // Someone added Italian and forgot to say so. Everything in it — every placeholder,
    // every plural — would go unchecked without a word.
    fs::write(
        root.join("locales/it.json"),
        "{\n  \"a\": \"Ciao {{utente}}\",\n  \"b\": \"Arrivederci\"\n}\n",
    )
    .unwrap();
    // A file that matches the template but is not a locale must not be mistaken for one.
    fs::write(
        root.join("locales/README.json"),
        "{\n  \"x\": \"notes\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (code, out, _) = check(root, &["--json"]);
    assert_eq!(
        code, 0,
        "an unlisted locale is a warning, not an error: {out}"
    );
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let f: Vec<&serde_json::Value> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "locale")
        .collect();
    assert_eq!(f.len(), 1, "{out}");
    assert_eq!(f[0]["locale"], "it");
    assert_eq!(f[0]["file"], "locales/it.json");
    assert_eq!(f[0]["severity"], "warning");
    assert!(
        f[0]["message"]
            .as_str()
            .unwrap()
            .contains("`it` is not in target_locales"),
        "{}",
        f[0]["message"]
    );
    // Listing it makes the warning go and the real bug in it appear.
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"it\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (code, out, _) = check(root, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(!out.contains("locale:"), "{out}");
    assert!(out.contains("placeholder name translated"), "{out}");
}

#[test]
fn an_android_qualifier_directory_is_not_a_locale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for d in ["values", "values-de", "values-night", "values-w600dp"] {
        fs::create_dir_all(root.join("res").join(d)).unwrap();
        fs::write(
            root.join("res").join(d).join("strings.xml"),
            "<resources><string name=\"a\">Hi</string></resources>\n",
        )
        .unwrap();
    }
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"android\"\npath = \"res/values/strings.xml\"\nlocale_path = \"res/values-{android_locale}/strings.xml\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (_, out, _) = check(root, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let locales: Vec<&serde_json::Value> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "locale")
        .collect();
    assert!(
        locales.is_empty(),
        "night and w600dp are qualifiers, not languages: {out}"
    );
}
