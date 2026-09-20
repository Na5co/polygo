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

#[test]
fn status_keys_lists_what_is_behind_each_count() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"fr\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"save\": \"Save\",\n  \"cancel\": \"Cancel\",\n  \"broken\": \"Broken\"\n}\n",
    )
    .unwrap();
    // A pre-existing German "cancel" (human), and "broken" is dropped by the model twice
    // → needs-review.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"cancel\": \"Abbrechen\"\n}\n",
    )
    .unwrap();
    let out = polygo()
        .current_dir(root)
        .args(["translate", "--locale", "de"])
        .env("POLYGO_NO_BACKOFF", "1")
        .env("POLYGO_MOCK_DROP_KEY", "broken")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Then the English "save" changes → stale in de, still new in fr.
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"save\": \"Save changes\",\n  \"cancel\": \"Cancel\",\n  \"broken\": \"Broken\"\n}\n",
    )
    .unwrap();

    let out = polygo()
        .current_dir(root)
        .args(["status", "--keys"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("    stale         save\n"), "{text}");
    assert!(text.contains("    edited        cancel\n"), "{text}");
    assert!(text.contains("    needs-review  broken  ·  "), "{text}");
    assert!(text.contains("    new           broken\n"), "{text}"); // fr
    assert!(!text.contains("up-to-date    "), "{text}");

    let out = polygo()
        .current_dir(root)
        .args(["status", "-k", "--locale", "de"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("  de "), "{text}");
    assert!(!text.contains("  fr "), "{text}");

    let out = polygo()
        .current_dir(root)
        .args(["status", "--keys", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["locales"]["de"]["keys"]["stale"],
        serde_json::json!(["save"])
    );
    assert_eq!(
        v["locales"]["de"]["keys"]["edited"],
        serde_json::json!(["cancel"])
    );
    assert_eq!(
        v["locales"]["de"]["keys"]["needs_review"][0]["key"],
        "broken"
    );
    assert!(v["locales"]["de"]["keys"]["needs_review"][0]["reason"].is_string());
    assert!(v["locales"]["fr"]["keys"]["new"].as_array().unwrap().len() == 3);
    assert!(v["locales"]["fr"]["keys"].get("stale").is_none());

    // Without --keys the JSON shape is unchanged; an unknown locale is an error.
    let out = polygo()
        .current_dir(root)
        .args(["status", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["locales"]["de"].get("keys").is_none());
    let out = polygo()
        .current_dir(root)
        .args(["status", "--locale", "xx"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
