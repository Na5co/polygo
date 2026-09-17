//! G2.2: a run killed halfway resumes without re-translating finished keys.
use std::fs;
use std::path::Path;
use std::process::Command;

fn polygo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
}

fn xcstrings_with(keys: &[&str]) -> String {
    let mut s = String::from("{\n  \"sourceLanguage\" : \"en\",\n  \"strings\" : {\n");
    for (i, k) in keys.iter().enumerate() {
        s.push_str(&format!(
            "    \"{k}\" : {{\n      \"localizations\" : {{\n        \"en\" : {{\n          \"stringUnit\" : {{\n            \"state\" : \"translated\",\n            \"value\" : \"{k} text\"\n          }}\n        }}\n      }}\n    }}{}\n",
            if i + 1 < keys.len() { "," } else { "" }
        ));
    }
    s.push_str("  },\n  \"version\" : \"1.0\"\n}");
    s
}

fn setup(root: &Path) {
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\nbatch_size = 2\n\n[[files]]\nformat = \"xcstrings\"\npath = \"Localizable.xcstrings\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Localizable.xcstrings"),
        xcstrings_with(&["k1", "k2", "k3", "k4", "k5", "k6"]),
    )
    .unwrap();
}

fn translated_keys(log: &Path) -> Vec<String> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn resume_after_kill() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let log = root.join("mock.log");

    // First run: the mock provider panics after 3 requests → batch 1 (k1,k2) is
    // written and locked, batch 2 dies mid-flight.
    let out = polygo()
        .current_dir(root)
        .env("POLYGO_MOCK_PANIC_AFTER", "3")
        .env("POLYGO_MOCK_LOG", &log)
        .arg("translate")
        .output()
        .unwrap();
    assert!(!out.status.success(), "first run must die");
    let lock = fs::read_to_string(root.join("polygo.lock")).expect("lock written incrementally");
    assert!(
        lock.contains("[keys.k1.locales.de]") && lock.contains("[keys.k2.locales.de]"),
        "{lock}"
    );
    assert!(
        !lock.contains("[keys.k3.locales.de]"),
        "k3 must not be locked:\n{lock}"
    );
    let file = fs::read_to_string(root.join("Localizable.xcstrings")).unwrap();
    assert!(
        file.contains("⟦de⟧ k1 text") && file.contains("⟦de⟧ k2 text"),
        "{file}"
    );
    assert!(!file.contains("⟦de⟧ k3 text"));
    let first = translated_keys(&log);
    assert_eq!(first, vec!["k1", "k2", "k3"]);

    // Second run: finishes the job and never touches k1/k2 again.
    fs::remove_file(&log).unwrap();
    let out = polygo()
        .current_dir(root)
        .env("POLYGO_MOCK_LOG", &log)
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let second = translated_keys(&log);
    assert_eq!(second, vec!["k3", "k4", "k5", "k6"]);
    let file = fs::read_to_string(root.join("Localizable.xcstrings")).unwrap();
    for k in ["k1", "k2", "k3", "k4", "k5", "k6"] {
        assert!(
            file.contains(&format!("⟦de⟧ {k} text")),
            "{k} missing:\n{file}"
        );
    }
    // Third run: nothing to do, provider not called at all.
    fs::remove_file(&log).unwrap();
    let out = polygo()
        .current_dir(root)
        .env("POLYGO_MOCK_LOG", &log)
        .arg("translate")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!log.exists() || translated_keys(&log).is_empty());
    // And the file is still valid Xcode JSON with locales in sorted order.
    let doc = polygo::formats::xcstrings::parse(&file).unwrap();
    let locs = doc.root["strings"]["k1"]["localizations"]
        .as_object()
        .unwrap();
    assert_eq!(locs.keys().collect::<Vec<_>>(), vec!["de", "en"]);
}

#[test]
fn translate_writes_android_and_json_locale_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("res/values")).unwrap();
    fs::create_dir_all(root.join("res/values-de")).unwrap();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("res/values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"hello\">Hello</string>\n    <string name=\"app\" translatable=\"false\">Polygo</string>\n    <string name=\"bye\">Bye</string>\n</resources>\n",
    )
    .unwrap();
    // Existing partial German file: hello already translated by a human.
    fs::write(
        root.join("res/values-de/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"hello\">Hallo</string>\n</resources>\n",
    )
    .unwrap();
    fs::write(root.join("locales/en.json"), "{\n  \"nav\": {\n    \"home\": \"Home\",\n    \"about\": \"About\"\n  },\n  \"cta\": \"Sign up\"\n}\n").unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"fr\"]\n\n[[files]]\nformat = \"android\"\npath = \"res/values/strings.xml\"\nlocale_path = \"res/values-{android_locale}/strings.xml\"\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = polygo()
        .current_dir(root)
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let de = fs::read_to_string(root.join("res/values-de/strings.xml")).unwrap();
    assert!(
        de.contains("<string name=\"hello\">Hallo</string>"),
        "human translation kept:\n{de}"
    );
    assert!(
        de.contains("<string name=\"bye\">⟦de⟧ Bye</string>"),
        "{de}"
    );
    assert!(
        !de.contains("Polygo"),
        "non-translatable must not be copied:\n{de}"
    );
    let fr = fs::read_to_string(root.join("res/values-fr/strings.xml")).unwrap();
    assert!(
        fr.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n"),
        "{fr}"
    );
    assert!(
        fr.contains("    <string name=\"hello\">⟦fr⟧ Hello</string>\n"),
        "{fr}"
    );

    let de_json = fs::read_to_string(root.join("locales/de.json")).unwrap();
    assert_eq!(
        de_json,
        "{\n  \"nav\": {\n    \"home\": \"⟦de⟧ Home\",\n    \"about\": \"⟦de⟧ About\"\n  },\n  \"cta\": \"⟦de⟧ Sign up\"\n}\n"
    );
    // Second run is a no-op and leaves files byte-identical.
    let out = polygo()
        .current_dir(root)
        .arg("translate")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        fs::read_to_string(root.join("locales/de.json")).unwrap(),
        de_json
    );
    assert_eq!(
        fs::read_to_string(root.join("res/values-de/strings.xml")).unwrap(),
        de
    );
}
