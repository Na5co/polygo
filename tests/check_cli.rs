//! G3.4: `polygo check` exit codes, JSON output, --strict and --fix.
use std::fs;
use std::path::Path;
use std::process::Command;

fn polygo(root: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_polygo"));
    c.current_dir(root)
        .env("POLYGO_CONFIG_DIR", root.join("cfg"));
    c
}

fn json_project(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"hello\": \"Hello, {{name}}!\",\n  \"items_one\": \"{{count}} item\",\n  \"items_other\": \"{{count}} items\",\n  \"ok\": \"OK\",\n  \"bye\": \"Goodbye\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"hello\": \"Hallo, {{name}}!\",\n  \"items_one\": \"{{count}} Element\",\n  \"items_other\": \"{{count}} Elemente\",\n  \"ok\": \"OK\",\n  \"bye\": \"Tschüss\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
}

#[test]
fn check_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    json_project(root);

    // Clean project → 0.
    let out = polygo(root).arg("check").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Break a placeholder and an i18next plural group → errors → 1, keys named.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"hello\": \"Hallo, {name}!\",\n  \"items_one\": \"{{count}} Element\",\n  \"ok\": \"OK\",\n  \"bye\": \"Goodbye\"\n}\n",
    )
    .unwrap();
    let out = polygo(root).arg("check").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("hello") && text.contains("placeholders"),
        "{text}"
    );
    assert!(text.contains("items") && text.contains("plural"), "{text}");
    assert!(text.contains("bye") && text.contains("identical"), "{text}");

    // JSON output carries severity per finding.
    let out = polygo(root).args(["check", "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().unwrap();
    assert!(
        findings.iter().any(|f| f["key"] == "hello"
            && f["severity"] == "error"
            && f["code"] == "placeholders")
    );
    assert!(
        findings
            .iter()
            .any(|f| f["key"] == "bye" && f["severity"] == "warning" && f["code"] == "identical")
    );
    assert_eq!(v["errors"].as_u64().unwrap(), 2);
    assert_eq!(v["warnings"].as_u64().unwrap(), 1);

    // Warnings alone → 0, unless --strict.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"hello\": \"Hallo, {{name}}!\",\n  \"items_one\": \"{{count}} Element\",\n  \"items_other\": \"{{count}} Elemente\",\n  \"ok\": \"OK\",\n  \"bye\": \"Goodbye\"\n}\n",
    )
    .unwrap();
    assert_eq!(
        polygo(root).arg("check").output().unwrap().status.code(),
        Some(0)
    );
    assert_eq!(
        polygo(root)
            .args(["check", "--strict"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );

    // --fix re-translates only the offending keys, then the check passes.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"hello\": \"Hallo, {name}!\",\n  \"items_one\": \"{{count}} Element\",\n  \"items_other\": \"{{count}} Elemente\",\n  \"ok\": \"OK\",\n  \"bye\": \"Tschüss\"\n}\n",
    )
    .unwrap();
    let log = root.join("mock.log");
    let out = polygo(root)
        .env("POLYGO_MOCK_LOG", &log)
        .args(["check", "--fix"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let calls: Vec<String> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(calls, vec!["hello"], "only the broken key is re-translated");
    let de = fs::read_to_string(root.join("locales/de.json")).unwrap();
    assert!(de.contains("\"hello\": \"⟦de⟧ Hello, {{name}}!\""), "{de}");
    assert!(
        de.contains("\"bye\": \"Tschüss\""),
        "untouched keys stay:\n{de}"
    );
    assert_eq!(
        polygo(root).arg("check").output().unwrap().status.code(),
        Some(0)
    );
}

#[test]
fn check_finds_xcstrings_plural_and_android_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("Localizable.xcstrings"),
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "%lld files" : {
      "localizations" : {
        "en" : {
          "variations" : {
            "plural" : {
              "one" : { "stringUnit" : { "state" : "translated", "value" : "%lld file" } },
              "other" : { "stringUnit" : { "state" : "translated", "value" : "%lld files" } }
            }
          }
        },
        "ru" : {
          "variations" : {
            "plural" : {
              "one" : { "stringUnit" : { "state" : "translated", "value" : "%lld файл" } },
              "other" : { "stringUnit" : { "state" : "translated", "value" : "%lld файлов" } }
            }
          }
        }
      }
    },
    "Hello %@" : {
      "localizations" : {
        "ru" : { "stringUnit" : { "state" : "translated", "value" : "Привет" } }
      }
    }
  },
  "version" : "1.0"
}"#,
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"ru\"]\n\n[[files]]\nformat = \"xcstrings\"\npath = \"Localizable.xcstrings\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = polygo(root).args(["check", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().unwrap();
    assert!(
        findings.iter().any(|f| f["key"] == "%lld files"
            && f["code"] == "plural"
            && f["message"].as_str().unwrap().contains("few")),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f["key"] == "Hello %@" && f["code"] == "placeholders"),
        "{findings:?}"
    );
}
