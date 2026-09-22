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

#[test]
fn baseline_hides_known_findings_and_fails_only_on_new_ones() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"a\": \"Hello {{name}}\",\n  \"b\": \"Settings\",\n  \"c\": \"Save\"\n}\n",
    )
    .unwrap();
    // Two problems today: a placeholder error and an identical warning.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"a\": \"Hallo {{nome}}\",\n  \"b\": \"Settings\",\n  \"c\": \"Speichern\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (code, _, _) = check(root, &[]);
    assert_eq!(code, 1);
    let (code, out, _) = check(root, &["--write-baseline"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("with 2 finding(s)"), "{out}");
    let b: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("polygo-baseline.json")).unwrap())
            .unwrap();
    assert_eq!(b["findings"].as_array().unwrap().len(), 2);
    assert_eq!(b["findings"][0]["code"], "placeholders");
    assert!(
        b["findings"][0].get("line").is_none(),
        "lines must not be pinned"
    );

    // Same state: clean run, exit 0, the baseline line says what is hidden.
    let (code, out, _) = check(root, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("check: ok"), "{out}");
    assert!(
        out.contains("baseline: 2 known finding(s) not shown"),
        "{out}"
    );
    let (code, _, _) = check(root, &["--no-baseline"]);
    assert_eq!(code, 1);

    // A new regression is reported and fails; the old ones stay hidden.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"a\": \"Hallo {{nome}}\",\n  \"b\": \"Settings\",\n  \"c\": \"Speichern {{x}}\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(root, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("  c  [de]  placeholders:"), "{out}");
    assert!(!out.contains("  a  [de]  placeholders:"), "{out}");
    assert!(out.contains("1 error(s), 0 warning(s)"), "{out}");
    let (_, out, _) = check(root, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["findings"].as_array().unwrap().len(), 1);
    assert_eq!(v["baseline"]["known"], 2);
    assert_eq!(v["baseline"]["stale"], 0);

    // Fixing a known one makes its entry stale, which the output says.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"a\": \"Hallo {{name}}\",\n  \"b\": \"Settings\",\n  \"c\": \"Speichern\"\n}\n",
    )
    .unwrap();
    let (code, out, _) = check(root, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("1 known finding(s) not shown, 1 entry no longer match (fixed?)"),
        "{out}"
    );
}

#[test]
fn inconsistent_terms_and_per_code_ignores() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"a\": \"Cancel\",\n  \"b\": \"Cancel\",\n  \"c\": \"Cancel\",\n  \"d\": \"Cancel\",\n  \"e\": \"None\",\n  \"f\": \"None\",\n  \"g\": \"Cancel\",\n  \"long\": \"Settings\",\n  \"brand\": \"Polygo\"\n}\n",
    )
    .unwrap();
    // c is the odd one out; g is untranslated (identical, not a vote); e/f differ only by
    // gender agreement; long is 2.5x too long; brand is identical.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"a\": \"Abbrechen\",\n  \"b\": \"abbrechen\",\n  \"c\": \"Abbruch\",\n  \"d\": \"Abbrechen\",\n  \"e\": \"Keine\",\n  \"f\": \"Keiner\",\n  \"g\": \"Cancel\",\n  \"long\": \"Einstellungen und noch viel mehr Text hier\",\n  \"brand\": \"Polygo\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (_, out, _) = check(root, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let by_code = |code: &str| -> Vec<String> {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["code"] == code)
            .map(|f| f["key"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(by_code("inconsistent"), ["c"], "{out}");
    let m = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "inconsistent")
        .unwrap();
    assert_eq!(
        m["message"],
        "`Cancel` is `Abbrechen` in 3 other key(s), here `Abbruch`"
    );
    assert_eq!(by_code("length"), ["long"]);
    assert_eq!(by_code("identical"), ["brand", "g"]);

    // [keys] ignore silences one code for matching keys, nothing else.
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[keys]\nignore = { \"brand\" = [\"identical\"], \"lo*\" = [\"length\"], \"c\" = [\"inconsistent\"] }\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let (_, out, err) = check(root, &["--json"]);
    assert!(!err.contains("unknown key"), "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let codes: Vec<(String, String)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["key"].as_str().unwrap().to_string(),
                f["code"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(codes, [("g".to_string(), "identical".to_string())], "{out}");
    assert_eq!(v["warnings"], 1);
}

#[test]
fn polygo_ignore_directive_in_a_comment() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("App")).unwrap();
    fs::write(
        root.join("App/Localizable.xcstrings"),
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "ACME" : {
      "comment" : "Brand, polygo:ignore=identical,length stays",
      "localizations" : {
        "de" : { "stringUnit" : { "state" : "translated", "value" : "ACME" } }
      }
    },
    "Save" : {
      "localizations" : {
        "de" : { "stringUnit" : { "state" : "needs_review", "value" : "Save" } }
      }
    }
  },
  "version" : "1.0"
}
"#,
    )
    .unwrap();
    let (_, out, _) = check(root, &["App/Localizable.xcstrings", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let codes: Vec<(String, String)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["key"].as_str().unwrap().to_string(),
                f["code"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        codes,
        [
            ("Save".to_string(), "identical".to_string()),
            ("Save".to_string(), "state".to_string())
        ],
        "{out}"
    );
    assert_eq!(
        polygo::core::directives(Some("polygo:ignore=identical,length stays")).ignore,
        ["identical", "length"]
    );
}

#[test]
fn sarif_output_is_well_formed() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(
        dir.path().join("locales/en.json"),
        "{\n  \"a\": \"Hi {{name}}\"\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("locales/de.json"),
        "{\n  \"a\": \"Hallo {{nome}}\"\n}\n",
    )
    .unwrap();
    let (code, out, err) = check(dir.path(), &["locales", "--sarif"]);
    assert_eq!(code, 1);
    assert!(
        err.is_empty(),
        "sarif must be the only stdout/stderr content: {err}"
    );
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["version"], "2.1.0");
    let run = &v["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "polygo");
    assert!(run["tool"]["driver"]["rules"].as_array().unwrap().len() >= 20);
    let r = &run["results"][0];
    assert_eq!(r["ruleId"], "polygo/placeholders");
    assert_eq!(r["level"], "error");
    assert_eq!(
        r["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "locales/de.json"
    );
    assert_eq!(
        r["locations"][0]["physicalLocation"]["region"]["startLine"],
        2
    );
    assert_eq!(
        r["partialFingerprints"]["polygo/v1"],
        "locales/de.json:a:de:placeholders"
    );
    assert!(
        r["message"]["text"]
            .as_str()
            .unwrap()
            .starts_with("a [de]: missing {{name}}")
    );
    // The Action's check.sh writes it next to the annotations when asked.
    let sarif = dir.path().join("out.sarif");
    let out = Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/action/check.sh"))
        .current_dir(dir.path())
        .env("POLYGO_BIN", env!("CARGO_BIN_EXE_polygo"))
        .env("POLYGO_PATH", "locales")
        .env("POLYGO_SARIF", &sarif)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&sarif).unwrap()).unwrap();
    assert_eq!(v["runs"][0]["results"].as_array().unwrap().len(), 1);
}

#[test]
fn fix_only_retranslates_what_a_model_can_cure() {
    // An Android project whose source file has an escape error and a duplicate key, plus
    // a real placeholder error in German: --fix must touch only the German key, and must
    // not ask the engine to translate into the source locale.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let res = root.join("res");
    fs::create_dir_all(res.join("values")).unwrap();
    fs::create_dir_all(res.join("values-de")).unwrap();
    fs::write(
        res.join("values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"bad\">Don't</string>\n    <string name=\"dup\">One</string>\n    <string name=\"dup\">Two</string>\n    <string name=\"hello\">Hello %1$s</string>\n</resources>\n",
    )
    .unwrap();
    fs::write(
        res.join("values-de/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"hello\">Hallo</string>\n</resources>\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"android\"\npath = \"res/values/strings.xml\"\nlocale_path = \"res/values-{android_locale}/strings.xml\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["check", "--fix"])
        .env("POLYGO_CONFIG_DIR", root.join("cfg"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(err.contains("re-translating 1 key(s)"), "{err}\n{text}");
    assert!(!err.contains("locale `en`"), "{err}");
    let de = fs::read_to_string(res.join("values-de/strings.xml")).unwrap();
    assert!(de.contains("⟦de⟧ Hello %1$s"), "{de}");
    // The source-file problems remain and still fail the run.
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text.contains("escape:") && text.contains("duplicate:"),
        "{text}"
    );
    assert!(!text.contains("hello  [de]  placeholders"), "{text}");
}

#[test]
fn locale_flag_is_validated_and_explain_prints_the_table() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(dir.path().join("locales/en.json"), "{\n  \"a\": \"A\"\n}\n").unwrap();
    fs::write(
        dir.path().join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n",
    )
    .unwrap();
    let (code, _, err) = check(dir.path(), &["--locale", "xx"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("locale `xx` is not in target_locales (de)"),
        "{err}"
    );
    let (code, out, _) = check(dir.path(), &["--explain", "link"]);
    assert_eq!(code, 0);
    assert!(
        out.starts_with("link · Link changed · warning by default\n"),
        "{out}"
    );
    let (_, out, _) = check(dir.path(), &["--explain", "all"]);
    assert!(out.matches(" by default").count() >= 22, "{out}");
    let (code, _, err) = check(dir.path(), &["--explain", "nope"]);
    assert_eq!(code, 1);
    assert!(err.contains("unknown code `nope`"), "{err}");
}

#[test]
fn file_level_findings_carry_the_file_prefix_in_multi_file_projects() {
    // Two [[files]]: an escape finding in the second one must be keyed
    // `res2/values/strings.xml:bad`, so polygo:ignore and the baseline resolve to that file.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for r in ["res1", "res2"] {
        fs::create_dir_all(root.join(r).join("values")).unwrap();
    }
    fs::write(
        root.join("res1/values/strings.xml"),
        "<resources>\n    <string name=\"ok\">Fine</string>\n</resources>\n",
    )
    .unwrap();
    fs::write(root.join("res2/values/strings.xml"), "<resources>\n    <!-- polygo:ignore=escape -->\n    <string name=\"bad\">Don't</string>\n    <string name=\"bad2\">Won't</string>\n</resources>\n").unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"android\"\npath = \"res1/values/strings.xml\"\nlocale_path = \"res1/values-{android_locale}/strings.xml\"\n\n[[files]]\nformat = \"android\"\npath = \"res2/values/strings.xml\"\nlocale_path = \"res2/values-{android_locale}/strings.xml\"\n",
    )
    .unwrap();
    let (_, out, _) = check(root, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let keys: Vec<String> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["key"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys, ["res2/values/strings.xml:bad2"], "{out}");
}

#[test]
fn po_plural_forms_header_must_match_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let po = dir.path().join("locale");
    fs::create_dir_all(&po).unwrap();
    fs::write(
        po.join("en.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\nmsgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n",
    )
    .unwrap();
    // Russian with a Germanic header: wrong before any string is.
    fs::write(
        po.join("ru.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: ru\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\nmsgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"%d файл\"\nmsgstr[1] \"%d файлов\"\n",
    )
    .unwrap();
    // German with no header at all but plurals in the file: a warning.
    fs::write(
        po.join("de.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: de\\n\"\n\nmsgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"%d Datei\"\nmsgstr[1] \"%d Dateien\"\n",
    )
    .unwrap();
    let (code, out, _) = check(dir.path(), &["locale", "--json"]);
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let hdr: Vec<(String, String, String)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["key"] == "Plural-Forms")
        .map(|f| {
            (
                f["locale"].as_str().unwrap().to_string(),
                f["severity"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().chars().take(40).collect(),
            )
        })
        .collect();
    assert_eq!(
        hdr,
        [
            (
                "de".to_string(),
                "warning".to_string(),
                "no Plural-Forms header: gettext assumes ".to_string()
            ),
            (
                "ru".to_string(),
                "error".to_string(),
                "header says nplurals=2; ru needs 3 (one,".to_string()
            ),
        ],
        "{out}"
    );
    let ru = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["locale"] == "ru" && f["key"] == "Plural-Forms")
        .unwrap();
    assert_eq!(ru["file"], "locale/ru.po");
    assert_eq!(ru["line"], 4);
}
