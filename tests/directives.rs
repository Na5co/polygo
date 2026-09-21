//! Per-key directives in developer comments: polygo:skip, polygo:max=N.
use std::fs;
use std::process::Command;

fn run(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(args)
        .env("POLYGO_NO_BACKOFF", "1")
        .output()
        .unwrap()
}

#[test]
fn directives_parse() {
    use polygo::core::directives;
    assert!(directives(Some("polygo:skip")).skip);
    assert_eq!(
        directives(Some("Tab title, polygo:max=12")).max_chars,
        Some(12)
    );
    assert_eq!(
        directives(Some("polygo:max=8; polygo:skip")).max_chars,
        Some(8)
    );
    assert!(!directives(Some("Nothing to see")).skip);
    assert!(!directives(None).skip);
}

#[test]
fn skip_and_max_are_enforced_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("App")).unwrap();
    fs::write(
        root.join("App/Localizable.xcstrings"),
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "ACME Cloud" : {
      "comment" : "Product name. polygo:skip",
      "extractionState" : "stale",
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "ACME Cloud"
          }
        },
        "de" : {
          "stringUnit" : {
            "state" : "needs_review",
            "value" : "ACME Cloud"
          }
        }
      }
    },
    "Save" : {
      "comment" : "Toolbar button, polygo:max=6",
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Save"
          }
        },
        "de" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Änderungen speichern"
          }
        }
      }
    },
    "Open" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Open"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#,
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"xcstrings\"\npath = \"App/Localizable.xcstrings\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    // skip: not in status, not planned.
    let out = run(root, &["status"]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("2 units"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out = run(root, &["translate", "--dry-run"]);
    let plan = String::from_utf8_lossy(&out.stdout);
    assert!(plan.contains("Open") && !plan.contains("ACME"), "{plan}");
    // max: the existing 20-char German "Save" is a check error naming the limit.
    let out = run(root, &["check"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    // skip also silences the file-level checks (state, plural) for that key.
    assert!(
        !text.contains("ACME"),
        "polygo:skip ignored by a file-level check:\n{text}"
    );
    assert!(
        text.contains("Save") && text.contains("at most 6 (polygo:max)"),
        "{text}"
    );
    // max: the mock's "⟦de⟧ Save" (9 chars) violates max=6 → repaired?  The mock cannot
    // shorten, so it is quarantined rather than written.
    fs::write(
        root.join("App/Localizable.xcstrings"),
        fs::read_to_string(root.join("App/Localizable.xcstrings"))
            .unwrap()
            .replace("\"value\" : \"Änderungen speichern\"", "\"value\" : \"\""),
    )
    .unwrap();
    let out = run(root, &["translate"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Save") && err.contains("must fit in 6"),
        "{err}"
    );
    let text = fs::read_to_string(root.join("App/Localizable.xcstrings")).unwrap();
    assert!(
        text.contains("⟦de⟧ Open") && !text.contains("⟦de⟧ Save"),
        "{text}"
    );
    assert!(!text.contains("⟦de⟧ ACME"), "{text}");
}

#[test]
fn keys_skip_patterns_in_polygo_toml() {
    // i18next JSON has no comment field for `polygo:skip`; `[keys] skip` covers it (and
    // every other format) with globs over the key.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"title\": \"Photos\",\n  \"debug\": {\n    \"trace\": \"trace on\",\n    \"level\": \"level {{n}}\"\n  },\n  \"internal_id\": \"ID\",\n  \"logs_one\": \"{{count}} log\",\n  \"logs_other\": \"{{count}} logs\",\n  \"photos_one\": \"{{count}} photo\",\n  \"photos_other\": \"{{count}} photos\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"pl\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[keys]\nskip = [\"debug.*\", \"internal_*\", \"logs\"]\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = run(root, &["status"]);
    let status = String::from_utf8_lossy(&out.stdout);
    // title + 4 Polish forms of photos; debug.*, internal_id and the logs group are out.
    assert!(status.contains("5 units"), "{status}");
    assert!(
        status.contains("4 key(s) skipped by [keys] skip"),
        "{status}"
    );
    let out = run(root, &["translate", "--dry-run"]);
    let plan = String::from_utf8_lossy(&out.stdout);
    assert!(
        plan.contains("title") && plan.contains("photos#plural.few"),
        "{plan}"
    );
    assert!(
        !plan.contains("debug") && !plan.contains("internal") && !plan.contains("logs"),
        "{plan}"
    );
    let out = run(root, &["translate"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pl = fs::read_to_string(root.join("locales/pl.json")).unwrap();
    assert!(pl.contains("\"title\": \"⟦pl⟧ Photos\""), "{pl}");
    assert!(!pl.contains("debug") && !pl.contains("logs_"), "{pl}");
    // A skipped plural group is not a `check` error either, even though pl lacks its forms.
    let out = run(root, &["check"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    // A bad glob is a clear error, not a silent no-match.
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"pl\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[keys]\nskip = [\"debug.[\"]\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = run(root, &["status"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("[keys] skip"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
