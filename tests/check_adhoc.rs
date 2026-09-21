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

#[test]
fn check_path_is_relative_to_the_root_and_annotations_stay_attachable() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    fs::create_dir_all(app.join("locales")).unwrap();
    fs::write(
        app.join("locales/en.json"),
        "{\n  \"greet\": \"Hi {{name}}\"\n}\n",
    )
    .unwrap();
    fs::write(
        app.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{nome}}\"\n}\n",
    )
    .unwrap();
    // -C app, path relative to it (used to be resolved against the cwd).
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(dir.path())
        .args(["-C", "app", "check", "locales", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["findings"][0]["file"], "app/locales/de.json");
    assert_eq!(v["findings"][0]["line"], 2);
    // A tree outside the cwd: the file is reported by an absolute path, not `en.json`.
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let (_, out, _) = check(
        &elsewhere,
        &[app.join("locales").to_str().unwrap(), "--json"],
    );
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let file = v["findings"][0]["file"].as_str().unwrap();
    assert!(
        file.ends_with("/app/locales/de.json") && file.starts_with('/'),
        "{file}"
    );
    // --strict --github: the warning becomes an error in the annotation AND the totals.
    fs::write(
        app.join("locales/en.json"),
        "{\n  \"greet\": \"Welcome back, {{name}}\"\n}\n",
    )
    .unwrap();
    fs::write(
        app.join("locales/de.json"),
        "{\n  \"greet\": \"Welcome back, {{name}}\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(&app, &["locales", "--github", "--strict"]);
    assert_eq!(code, 1);
    assert!(
        out.contains("::error file=locales/de.json,line=2,title=polygo identical [de]"),
        "{out}"
    );
    assert!(
        out.contains("polygo check: 1 error(s), 0 warning(s)"),
        "{out}"
    );
    let (code, out, _) = check(&app, &["locales", "--github"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("::warning file=") && out.contains("0 error(s), 1 warning(s)"),
        "{out}"
    );
    // Inside a configured project a path is a note, not a silent config bypass, and
    // --fix says the right thing.
    fs::write(
        app.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (_, _, err) = check(&app, &["locales"]);
    assert!(
        err.contains("polygo.toml settings ([keys] skip, length_ratio) do not apply"),
        "{err}"
    );
    let (code, _, err) = check(&app, &["locales", "--fix"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("run `polygo check --fix` without a path"),
        "{err}"
    );
}

#[test]
fn android_escapes_that_break_or_bend_the_build() {
    let dir = tempfile::tempdir().unwrap();
    let res = dir.path().join("res");
    fs::create_dir_all(res.join("values")).unwrap();
    fs::create_dir_all(res.join("values-de")).unwrap();
    fs::write(
        res.join("values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"a\">Don\\'t panic</string>\n    <string name=\"b\">\"Quoted 'apostrophe' is fine\"</string>\n    <string name=\"c\">Stray quote\"</string>\n    <string name=\"d\">Quotes in <a href='x' title=\"y\">tag attributes</a> pass</string>\n    <string name=\"e\"><![CDATA[Don't touch 'CDATA']]></string>\n    <string name=\"f\">@string/other</string>\n    <string-array name=\"g\">\n        <item>? really</item>\n        <item>fine</item>\n    </string-array>\n</resources>\n",
    )
    .unwrap();
    fs::write(
        res.join("values-de/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"a\">Keine Panik, das geht schon</string>\n    <string name=\"b\">Geht's? Nein</string>\n</resources>\n",
    )
    .unwrap();
    let (code, out, err) = check(dir.path(), &["res", "--json"]);
    assert_eq!(code, 1, "{out}{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let escapes: Vec<(String, String, String, u64)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "escape")
        .map(|f| {
            (
                f["key"].as_str().unwrap().to_string(),
                f["locale"].as_str().unwrap().to_string(),
                f["severity"].as_str().unwrap().to_string(),
                f["line"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        escapes,
        [
            ("b".into(), "de".into(), "error".into(), 4), // Geht's in the German file
            ("c".into(), "en".into(), "warning".into(), 5),
            ("g".into(), "en".into(), "error".into(), 9),
        ],
        "{out}"
    );
    let unescaped = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == "b" && f["locale"] == "de")
        .unwrap();
    assert_eq!(unescaped["file"], "res/values-de/strings.xml");
    assert!(
        unescaped["message"]
            .as_str()
            .unwrap()
            .contains("unescaped apostrophe")
    );
}

#[test]
fn orphans_fuzzy_states_and_coverage() {
    let dir = tempfile::tempdir().unwrap();
    // i18next: an orphan key, a plural form the locale needs (not an orphan), a missing key.
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(
        dir.path().join("locales/en.json"),
        "{\n  \"title\": \"Photos\",\n  \"photos_one\": \"{{count}} photo\",\n  \"photos_other\": \"{{count}} photos\",\n  \"later\": \"Later\"\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("locales/pl.json"),
        "{\n  \"title\": \"Zdjęcia\",\n  \"photos_one\": \"{{count}} zdjęcie\",\n  \"photos_few\": \"{{count}} zdjęcia\",\n  \"photos_many\": \"{{count}} zdjęć\",\n  \"photos_other\": \"{{count}} zdjęcia\",\n  \"old_button\": \"Stary\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(dir.path(), &["locales"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("warning locales/pl.json:7  old_button  [pl]  orphan: not in the source file"),
        "{out}"
    );
    assert!(
        !out.contains("photos_few"),
        "plural form flagged as orphan:\n{out}"
    );
    assert!(out.contains("coverage: pl 83% (1 of 6 missing)"), "{out}");
    let (_, out, _) = check(dir.path(), &["locales", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["coverage"]["pl"], serde_json::json!([5, 6]));

    // gettext: fuzzy.
    let po = dir.path().join("locale");
    fs::create_dir_all(&po).unwrap();
    fs::write(
        po.join("en.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\nmsgid \"Save\"\nmsgstr \"\"\n\nmsgid \"Open\"\nmsgstr \"\"\n",
    )
    .unwrap();
    fs::write(
        po.join("de.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: de\\n\"\n\n#, fuzzy\nmsgid \"Save\"\nmsgstr \"Speichern\"\n\nmsgid \"Open\"\nmsgstr \"Öffnen\"\n",
    )
    .unwrap();
    let (code, out, _) = check(dir.path(), &["locale"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("warning locale/de.po:6  Save  [de]  fuzzy: marked fuzzy"),
        "{out}"
    );
    assert!(!out.contains("coverage:"), "fully covered:\n{out}");

    // xcstrings: needs_review and a stale key.
    let xc = dir.path().join("App");
    fs::create_dir_all(&xc).unwrap();
    fs::write(
        xc.join("Localizable.xcstrings"),
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "Open" : {
      "localizations" : {
        "de" : {
          "stringUnit" : {
            "state" : "needs_review",
            "value" : "Öffnen"
          }
        }
      }
    },
    "Save" : {
      "extractionState" : "stale",
      "localizations" : {
        "de" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Sichern"
          }
        }
      }
    }
  },
  "version" : "1.0"
}
"#,
    )
    .unwrap();
    let (_, out, _) = check(dir.path(), &["App/Localizable.xcstrings"]);
    assert!(
        out.contains(
            "App/Localizable.xcstrings:6  Open  [de]  state: marked `needs_review` in Xcode"
        ),
        "{out}"
    );
    assert!(
        out.contains("App/Localizable.xcstrings:14  Save  [en]  state: extractionState is stale"),
        "{out}"
    );
}

#[test]
fn text_output_groups_a_dominating_code() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    let en: String = (1..=25)
        .map(|i| format!("  \"k{i}\": \"Settings page {i}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    fs::write(
        dir.path().join("locales/en.json"),
        format!("{{\n{en}\n}}\n"),
    )
    .unwrap();
    fs::copy(
        dir.path().join("locales/en.json"),
        dir.path().join("locales/de.json"),
    )
    .unwrap();
    let (code, out, _) = check(dir.path(), &["locales"]);
    assert_eq!(code, 0);
    assert_eq!(out.matches("identical:").count(), 20, "{out}");
    assert!(
        out.contains("… 5 more identical (first 20 of each shown; --all lists every one"),
        "{out}"
    );
    assert!(out.contains("0 error(s), 25 warning(s)"), "{out}");
    let (_, out, _) = check(dir.path(), &["locales", "--all"]);
    assert_eq!(out.matches("identical:").count(), 25, "{out}");
    assert!(!out.contains("more identical"), "{out}");
    let (_, out, _) = check(dir.path(), &["locales", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["findings"].as_array().unwrap().len(), 25);
}

#[test]
fn seven_more_bug_classes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Android: array length, unnumbered args, duplicate key.
    let res = root.join("res");
    fs::create_dir_all(res.join("values")).unwrap();
    fs::create_dir_all(res.join("values-de")).unwrap();
    fs::write(
        res.join("values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"both\">%1$s of %2$s</string>\n    <string name=\"raw\" formatted=\"false\">%s %s</string>\n    <string-array name=\"sort\">\n        <item>Newest</item>\n        <item>Oldest</item>\n        <item>Name</item>\n    </string-array>\n    <string name=\"dup\">One</string>\n    <string name=\"dup\">Two</string>\n</resources>\n",
    )
    .unwrap();
    fs::write(
        res.join("values-de/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"both\">%s von %s</string>\n    <string-array name=\"sort\">\n        <item>Neueste</item>\n        <item>Älteste</item>\n    </string-array>\n</resources>\n",
    )
    .unwrap();
    let (_, out, _) = check(root, &["res", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let codes = |v: &serde_json::Value, key: &str, locale: &str| -> Vec<String> {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["key"] == key && f["locale"] == locale)
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(
        codes(&v, "both", "de").contains(&"placeholders".into()),
        "{out}"
    );
    assert!(
        codes(&v, "raw", "en").is_empty(),
        "formatted=false must pass:\n{out}"
    );
    assert_eq!(codes(&v, "sort", "de"), ["array"], "{out}");
    assert_eq!(codes(&v, "dup", "en"), ["duplicate"], "{out}");
    let dup = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "duplicate")
        .unwrap();
    assert_eq!(dup["message"], "key appears more than once in this file");

    // JSON: mojibake, invisible, link, brackets, entities, glossary.
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"cafe\": \"Café open\",\n  \"zw\": \"Zero width\",\n  \"help\": \"See https://a.io/help now\",\n  \"paren\": \"Save (all)\",\n  \"amp\": \"Terms &amp; conditions\",\n  \"brand\": \"Sign in to Polygo\",\n  \"fine\": \"All good\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"cafe\": \"CafÃ© offen\",\n  \"zw\": \"Null\\u200bbreit\",\n  \"help\": \"Siehe https://a.io/hilfe jetzt\",\n  \"paren\": \"Alles speichern (alle\",\n  \"amp\": \"AGB &amp;amp; Bedingungen\",\n  \"brand\": \"Bei Poligo anmelden\",\n  \"fine\": \"Alles gut\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    fs::write(
        root.join("glossary.toml"),
        "do_not_translate = [\"Polygo\"]\n\n[terms.de]\n\"Sign in\" = \"Anmelden\"\n",
    )
    .unwrap();
    let (_, out, _) = check(root, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(codes(&v, "cafe", "de"), ["encoding"], "{out}");
    assert_eq!(codes(&v, "zw", "de"), ["invisible"], "{out}");
    assert_eq!(codes(&v, "help", "de"), ["link"], "{out}");
    assert_eq!(codes(&v, "paren", "de"), ["brackets"], "{out}");
    assert_eq!(codes(&v, "amp", "de"), ["entities"], "{out}");
    assert_eq!(codes(&v, "brand", "de"), ["glossary"], "{out}");
    assert!(codes(&v, "fine", "de").is_empty(), "{out}");
    let g = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "glossary")
        .unwrap();
    assert_eq!(g["message"], "`Polygo` must stay untranslated");
    assert_eq!(g["severity"], "error");
    let (code, out, _) = check(root, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("looks like text saved in the wrong encoding (`Ã©`)"),
        "{out}"
    );
}
