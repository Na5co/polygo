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
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
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
