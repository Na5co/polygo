//! G5.1: Flutter .arb — byte-stable round-trip, units with @meta descriptions, locale-file writing,
//! and an end-to-end translate through the engine.
use polygo::formats::arb;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn corpus() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/arb");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "arb"))
        .collect();
    files.sort();
    assert!(files.len() >= 3);
    files
}

#[test]
fn roundtrip_arb() {
    for path in corpus() {
        let original = fs::read_to_string(&path).unwrap();
        let doc = arb::parse(&original).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            arb::serialize(&doc),
            original,
            "{} not byte-stable",
            path.display()
        );
        let units = arb::units(&doc);
        let expected =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&original)
                .unwrap()
                .keys()
                .filter(|k| !k.starts_with('@'))
                .count();
        assert_eq!(units.len(), expected, "{}", path.display());
        assert!(units.iter().all(|u| !u.key.starts_with('@')));
    }
    // Descriptions from @meta become unit comments; placeholders are mentioned.
    let ente = arb::parse(
        &fs::read_to_string(corpus().iter().find(|p| p.ends_with("ente.arb")).unwrap()).unwrap(),
    )
    .unwrap();
    let with_comment = arb::units(&ente)
        .iter()
        .filter(|u| u.comment.is_some())
        .count();
    assert!(with_comment > 100, "{with_comment}");
}

#[test]
fn arb_locale_file_is_built_in_source_order_with_meta_stripped() {
    let src = "{\n  \"@@locale\": \"en\",\n  \"hello\": \"Hello {name}\",\n  \"@hello\": {\n    \"description\": \"Greeting\",\n    \"placeholders\": {\n      \"name\": {\n        \"type\": \"String\"\n      }\n    }\n  },\n  \"items\": \"{count, plural, one {# item} other {# items}}\",\n  \"@items\": {\n    \"placeholders\": {\n      \"count\": {}\n    }\n  },\n  \"bye\": \"Bye\"\n}\n";
    let doc = arb::parse(src).unwrap();
    let units = arb::units(&doc);
    assert_eq!(
        units[0].comment.as_deref(),
        Some("Greeting (placeholders: name)")
    );
    let mut values = std::collections::BTreeMap::new();
    values.insert("hello".to_string(), "Hallo {name}".to_string());
    values.insert("bye".to_string(), "Tschüss".to_string());
    let out = arb::build_locale_file(src, None, "de", &values);
    assert_eq!(
        out,
        "{\n  \"@@locale\": \"de\",\n  \"hello\": \"Hallo {name}\",\n  \"bye\": \"Tschüss\"\n}\n"
    );
    // Existing target values survive and new ones merge in source order; target-only keys are kept.
    let existing = "{\n  \"@@locale\": \"de\",\n  \"items\": \"{count, plural, one {# Element} other {# Elemente}}\",\n  \"legacy\": \"Alt\"\n}\n";
    let out = arb::build_locale_file(src, Some(existing), "de", &values);
    assert_eq!(
        out,
        "{\n  \"@@locale\": \"de\",\n  \"hello\": \"Hallo {name}\",\n  \"items\": \"{count, plural, one {# Element} other {# Elemente}}\",\n  \"bye\": \"Tschüss\",\n  \"legacy\": \"Alt\"\n}\n"
    );
}

#[test]
fn arb_end_to_end_translate_and_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("lib/l10n")).unwrap();
    fs::write(
        root.join("l10n.yaml"),
        "arb-dir: lib/l10n\ntemplate-arb-file: app_en.arb\n",
    )
    .unwrap();
    fs::write(root.join("lib/l10n/app_en.arb"), "{\n  \"@@locale\": \"en\",\n  \"title\": \"Chess\",\n  \"@title\": {\n    \"description\": \"App title\"\n  },\n  \"moves\": \"{n, plural, one {# move} other {# moves}}\"\n}\n").unwrap();
    fs::write(
        root.join("lib/l10n/app_fr.arb"),
        "{\n  \"@@locale\": \"fr\",\n  \"title\": \"Échecs\"\n}\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("init")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cfg = fs::read_to_string(root.join("polygo.toml"))
        .unwrap()
        .replace("kind = \"ollama\"", "kind = \"mock\"");
    fs::write(root.join("polygo.toml"), cfg).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fr = fs::read_to_string(root.join("lib/l10n/app_fr.arb")).unwrap();
    assert!(
        fr.contains("\"title\": \"Échecs\""),
        "human translation kept:\n{fr}"
    );
    assert!(
        fr.contains("\"moves\": \"⟦fr⟧ {n, plural, one {# move} other {# moves}}\""),
        "{fr}"
    );
    assert!(
        !fr.contains("\"@title\""),
        "meta must not be copied into locale files:\n{fr}"
    );
    let status = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["status", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(v["locales"]["fr"]["up_to_date"], 1);
    assert_eq!(v["locales"]["fr"]["edited"], 1);
    // check runs clean (plural forms complete for fr).
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("check")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
