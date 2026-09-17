//! G1.3: Android `strings.xml`: byte-stable round-trip, entry model, minimal edits, escapes.
use polygo::formats::android::{self, Kind};
use std::fs;
use std::path::PathBuf;

fn corpus() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/android");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
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
fn roundtrip_android() {
    for path in corpus() {
        let original = fs::read_to_string(&path).unwrap();
        let doc = android::parse(&original)
            .unwrap_or_else(|e| panic!("{}: parse failed: {e}", path.display()));
        assert_eq!(
            android::serialize(&doc),
            original,
            "{} is not byte-stable",
            path.display()
        );
        assert!(
            !doc.entries.is_empty(),
            "{}: no entries parsed",
            path.display()
        );
    }
}

#[test]
fn entry_model_matches_grep_counts() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/android/antennapod.xml");
    let text = fs::read_to_string(&path).unwrap();
    let doc = android::parse(&text).unwrap();
    let plurals = doc
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Plurals)
        .count();
    let non_translatable = doc.entries.iter().filter(|e| !e.translatable).count();
    assert_eq!(plurals, text.matches("<plurals").count());
    assert_eq!(
        non_translatable,
        text.matches("translatable=\"false\"").count()
    );
    let strings = doc
        .entries
        .iter()
        .filter(|e| e.kind == Kind::String)
        .count();
    assert_eq!(strings, text.matches("<string ").count());
    // Every plural item carries its quantity.
    for e in doc.entries.iter().filter(|e| e.kind == Kind::Plurals) {
        assert!(
            e.values.iter().all(|v| v.quantity.is_some()),
            "{}: item without quantity",
            e.name
        );
    }
}

#[test]
fn edit_is_minimal_and_reparses() {
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<resources>
    <!-- Greeting shown on the home screen -->
    <string name="hello">Hello, %1$s!</string>
    <string name="app_name" translatable="false">Polygo</string>
    <string name="html"><![CDATA[Tap <b>here</b>]]></string>
    <string name="empty"/>
    <plurals name="items">
        <item quantity="one">%d item</item>
        <item quantity="other">%d items</item>
    </plurals>
    <string-array name="days">
        <item>Mon</item>
        <item>Tue</item>
    </string-array>
</resources>
"#;
    let mut doc = android::parse(src).unwrap();
    assert_eq!(android::serialize(&doc), src);

    let hello = doc.entries.iter().position(|e| e.name == "hello").unwrap();
    assert_eq!(
        doc.entries[hello].comment.as_deref(),
        Some("Greeting shown on the home screen")
    );
    assert_eq!(doc.entries[hello].values[0].text(), "Hello, %1$s!");

    doc.set_text(hello, 0, "Hallo, %1$s!");
    let out = android::serialize(&doc);
    assert_eq!(out, src.replace("Hello, %1$s!", "Hallo, %1$s!"));

    // CDATA wrapper is preserved on edit; text view strips it.
    let html = doc.entries.iter().position(|e| e.name == "html").unwrap();
    assert_eq!(doc.entries[html].values[0].text(), "Tap <b>here</b>");
    doc.set_text(html, 0, "Tippe <b>hier</b>");
    assert!(android::serialize(&doc).contains("<![CDATA[Tippe <b>hier</b>]]>"));

    // Empty element gets a real value.
    let empty = doc.entries.iter().position(|e| e.name == "empty").unwrap();
    doc.set_text(empty, 0, "Filled");
    let out = android::serialize(&doc);
    assert!(
        out.contains(r#"<string name="empty">Filled</string>"#),
        "{out}"
    );

    // Plurals and arrays are addressable.
    let items = doc.entries.iter().position(|e| e.name == "items").unwrap();
    assert_eq!(
        doc.entries[items].values[1].quantity.as_deref(),
        Some("other")
    );
    let days = doc.entries.iter().position(|e| e.name == "days").unwrap();
    assert_eq!(doc.entries[days].kind, Kind::StringArray);
    assert_eq!(doc.entries[days].values.len(), 2);

    // Everything still parses and the edited values read back.
    let doc2 = android::parse(&out).unwrap();
    let hello2 = doc2.entries.iter().find(|e| e.name == "hello").unwrap();
    assert_eq!(hello2.values[0].text(), "Hallo, %1$s!");
}

#[test]
fn escapes_roundtrip() {
    for (raw, text) in [
        (r"It\'s %1$s", "It's %1$s"),
        (r#"Say \"hi\""#, "Say \"hi\""),
        (r"Line\nBreak", "Line\nBreak"),
        ("Tom &amp; Jerry &lt;3", "Tom & Jerry <3"),
        (r" nbsp", "\u{00A0}nbsp"),
        (r"\@at \?q", "@at ?q"),
    ] {
        assert_eq!(android::decode(raw), text, "decode {raw}");
        assert_eq!(
            android::decode(&android::encode(text)),
            text,
            "encode∘decode {text:?}"
        );
    }
    // Encoding keeps inline markup usable and escapes what Android requires.
    assert_eq!(
        android::encode("It's <b>bold</b> & more"),
        r"It\'s <b>bold</b> &amp; more"
    );
    assert_eq!(android::encode("?leading"), r"\?leading");
}
