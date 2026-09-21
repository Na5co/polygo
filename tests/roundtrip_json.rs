//! G1.4: i18next-style JSON: byte-stable round-trip, nested/flat key paths, minimal edits.
use polygo::formats::json;
use std::fs;
use std::path::PathBuf;

fn corpus() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/json");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && !p.to_string_lossy().ends_with(".mutations.json")
        })
        .collect();
    files.sort();
    assert!(
        files.len() >= 5,
        "corpus needs >= 5 files, found {}",
        files.len()
    );
    files
}

#[test]
fn roundtrip_json() {
    for path in corpus() {
        let original = fs::read_to_string(&path).unwrap();
        let doc = json::parse(&original)
            .unwrap_or_else(|e| panic!("{}: parse failed: {e}", path.display()));
        assert_eq!(
            json::serialize(&doc),
            original,
            "{} is not byte-stable",
            path.display()
        );
        assert!(!doc.entries.is_empty(), "{}: no entries", path.display());
        // Every entry's decoded text must agree with serde_json's view of the same file.
        let tree: serde_json::Value = serde_json::from_str(&original).unwrap();
        for e in &doc.entries {
            let mut node = &tree;
            for seg in &e.path {
                node = match seg.parse::<usize>() {
                    Ok(i) if node.is_array() => &node[i],
                    _ => &node[seg.as_str()],
                };
            }
            assert_eq!(
                node.as_str(),
                Some(e.text().as_str()),
                "{}: {}",
                path.display(),
                e.key()
            );
        }
    }
}

#[test]
fn nested_and_flat_paths_and_edits() {
    let src = "{\n  \"greeting\": \"Hello, {{name}}!\",\n  \"menu\": {\n    \"file\": \"File\",\n    \"edit\": {\n      \"undo\": \"Undo\"\n    }\n  },\n  \"flat.dotted.key\": \"Dotted\",\n  \"list\": [\"one\", \"two\"],\n  \"escaped\": \"Ellipsis\\u2026 \\\"quoted\\\" \\\\ back\",\n  \"count\": 3,\n  \"on\": true,\n  \"nothing\": null,\n  \"empty\": {}\n}\n";
    let mut doc = json::parse(src).unwrap();
    assert_eq!(json::serialize(&doc), src);

    let keys: Vec<String> = doc.entries.iter().map(|e| e.key()).collect();
    assert_eq!(
        keys,
        [
            "greeting",
            "menu.file",
            "menu.edit.undo",
            "flat.dotted.key",
            "list.0",
            "list.1",
            "escaped"
        ]
    );
    let esc = doc.entries.iter().find(|e| e.key() == "escaped").unwrap();
    assert_eq!(esc.text(), "Ellipsis\u{2026} \"quoted\" \\ back");
    let dotted = doc
        .entries
        .iter()
        .find(|e| e.key() == "flat.dotted.key")
        .unwrap();
    assert_eq!(dotted.path, vec!["flat.dotted.key".to_string()]);

    let i = doc
        .entries
        .iter()
        .position(|e| e.key() == "menu.edit.undo")
        .unwrap();
    doc.set_text(i, "Rückgängig \"jetzt\"");
    let out = json::serialize(&doc);
    assert_eq!(out, src.replace("\"Undo\"", "\"Rückgängig \\\"jetzt\\\"\""));
    let doc2 = json::parse(&out).unwrap();
    assert_eq!(
        doc2.entries
            .iter()
            .find(|e| e.key() == "menu.edit.undo")
            .unwrap()
            .text(),
        "Rückgängig \"jetzt\""
    );
}

#[test]
fn bom_and_crlf_are_preserved() {
    let src = "\u{FEFF}{\r\n  \"a\": \"b\"\r\n}\r\n";
    let mut doc = json::parse(src).unwrap();
    assert_eq!(json::serialize(&doc), src);
    doc.set_text(0, "c");
    assert_eq!(json::serialize(&doc), src.replace("\"b\"", "\"c\""));
}

#[test]
fn rejects_non_object_root_and_bad_json() {
    assert!(json::parse("[1,2]").is_err());
    assert!(json::parse("{\"a\": }").is_err());
    assert!(json::parse("{\"a\": \"b\"} trailing").is_err());
}

#[test]
fn empty_source_value_means_the_key_is_the_text() {
    // Open WebUI / Ghost style: "Deleted {{name}}": "" in en.json.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(
        root.join("locales/en.json"),
        "{\n  \"Deleted {{name}}\": \"\",\n  \"Save\": \"\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("locales/de.json"),
        "{\n  \"Deleted {{name}}\": \"{{name}} gelöscht\",\n  \"Save\": \"Speichern\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("check")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let cfg = polygo::config::Config::load(root).unwrap();
    let units = polygo::project::load_units(root, &cfg).unwrap();
    assert_eq!(units[0].source, "Deleted {{name}}");
}
