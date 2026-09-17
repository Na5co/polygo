//! G1.5: `polygo status` on a real project reports per-locale counts and respects the lockfile.
use std::fs;
use std::process::Command;

fn polygo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
}

#[test]
fn status_reports_counts_for_xcstrings_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"bg\"]\n\n[[files]]\nformat = \"xcstrings\"\npath = \"Localizable.xcstrings\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Localizable.xcstrings"),
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "cancel" : {
      "localizations" : {
        "de" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Abbrechen"
          }
        }
      }
    },
    "save" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Save"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#,
    )
    .unwrap();

    let out = polygo().current_dir(root).arg("status").output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    // No lockfile yet: every key is new for every locale.
    assert!(text.contains("de"), "{text}");
    assert!(text.contains("bg"), "{text}");
    // "cancel" already has a German translation (not ours) → edited; "save" → new.
    assert!(text.contains("new: 1"), "{text}");
    assert!(text.contains("edited: 1"), "{text}");

    let json = polygo()
        .current_dir(root)
        .args(["status", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["locales"]["de"]["new"], 1);
    assert_eq!(v["locales"]["de"]["edited"], 1);
    assert_eq!(v["locales"]["bg"]["new"], 2);
    assert_eq!(v["units"], 2);
}
