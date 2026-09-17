//! G5.2: gettext .po and .NET .resx/.resw: byte-stable round-trips, edits, inserts, new files, e2e.
use polygo::formats::{po, resx};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn corpus(sub: &str, ext: &[&str]) -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(sub);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| ext.contains(&e.to_str().unwrap()))
        })
        .collect();
    files.sort();
    files
}

#[test]
fn roundtrip_po() {
    let files = corpus("po", &["po"]);
    assert!(files.len() >= 4);
    for path in &files {
        let original = fs::read_to_string(path).unwrap();
        let doc = po::parse(&original).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            po::serialize(&doc),
            original,
            "{} not byte-stable",
            path.display()
        );
        assert!(
            doc.entries.len() > 300,
            "{}: {}",
            path.display(),
            doc.entries.len()
        );
        assert!(doc.header.contains("Content-Type"), "{}", path.display());
    }
    // Django de: msgctxt keys are distinct, plurals preserved but not units, translations decode.
    let de = po::parse(
        &fs::read_to_string(
            corpus("po", &["po"])
                .iter()
                .find(|p| p.ends_with("django_de.po"))
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(de.entries.iter().any(|e| e.ctxt.is_some()));
    assert!(de.entries.iter().any(|e| e.plural.is_some()));
    let vals = po::values(&de);
    assert!(vals.len() > 250, "{}", vals.len());
    let penpot = po::parse(
        &fs::read_to_string(
            corpus("po", &["po"])
                .iter()
                .find(|p| p.ends_with("penpot_de.po"))
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        po::values(&penpot)
            .values()
            .any(|v| v.len() > 100 && !v.contains('"')),
        "wrapped msgstr must join"
    );
    let keys: Vec<String> = po::units(&de).iter().map(|u| u.key.clone()).collect();
    assert!(
        keys.iter().any(|k| k.contains('\u{4}')),
        "ctxt keys use the gettext separator"
    );
}

#[test]
fn po_edit_insert_and_new_file() {
    let src = "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\n#. Button label\n#: src/app.py:12\nmsgid \"Save\"\nmsgstr \"\"\n\nmsgctxt \"menu\"\nmsgid \"Open\"\nmsgstr \"\"\n\nmsgid \"Line one\\n\"\n\"line two\"\nmsgstr \"\"\n";
    let mut doc = po::parse(src).unwrap();
    assert_eq!(po::serialize(&doc), src);
    let units = po::units(&doc);
    assert_eq!(units.len(), 3);
    assert_eq!(
        units[0].comment.as_deref(),
        Some("Button label · refs: src/app.py:12")
    );
    assert_eq!(units[1].key, "menu\u{4}Open");
    assert_eq!(units[2].source, "Line one\nline two");

    let i = doc.index_of("Save").unwrap();
    doc.set_msgstr(i, "Speichern");
    let j = doc.index_of("Line one\nline two").unwrap();
    doc.set_msgstr(j, "Zeile eins\nZeile zwei");
    let out = po::serialize(&doc);
    assert!(
        out.contains("msgid \"Save\"\nmsgstr \"Speichern\"\n"),
        "{out}"
    );
    assert!(
        out.contains("msgstr \"\"\n\"Zeile eins\\n\"\n\"Zeile zwei\"\n"),
        "{out}"
    );
    let again = po::parse(&out).unwrap();
    assert_eq!(
        po::values(&again)["Line one\nline two"],
        "Zeile eins\nZeile zwei"
    );

    doc.insert(
        Some("menu"),
        "Close",
        "Schließen",
        Some("refs: src/app.py:20"),
    );
    let out = po::serialize(&doc);
    assert!(
        out.ends_with(
            "\n#: src/app.py:20\nmsgctxt \"menu\"\nmsgid \"Close\"\nmsgstr \"Schließen\"\n"
        ),
        "{out}"
    );
    assert_eq!(
        po::values(&po::parse(&out).unwrap())["menu\u{4}Close"],
        "Schließen"
    );

    let fresh = po::new_locale_file(&doc, "ru");
    assert!(fresh.contains("\"Language: ru\\n\""), "{fresh}");
    assert!(fresh.contains("nplurals=3"), "{fresh}");
    assert!(po::parse(&fresh).unwrap().entries.len() == 1);
}

#[test]
fn roundtrip_resx() {
    let files = corpus("resx", &["resx", "resw"]);
    assert!(files.len() >= 5);
    for path in &files {
        let original = fs::read_to_string(path).unwrap();
        let doc = resx::parse(&original).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            resx::serialize(&doc),
            original,
            "{} not byte-stable",
            path.display()
        );
        // Count text <data> elements outside XML comments (the standard resx header
        // comment contains example <data> lines).
        let mut n_text_data = 0;
        let mut in_comment = false;
        for l in original.lines() {
            let t = l.trim_start();
            if t.starts_with("<!--") {
                in_comment = true;
            }
            if !in_comment
                && t.starts_with("<data ")
                && !t.contains(" type=\"")
                && !t.contains(" mimetype=\"")
            {
                n_text_data += 1;
            }
            if l.contains("-->") {
                in_comment = false;
            }
        }
        assert_eq!(doc.entries.len(), n_text_data, "{}", path.display());
    }
    let en = resx::parse(
        &fs::read_to_string(
            corpus("resx", &["resw"])
                .iter()
                .find(|p| p.ends_with("files_en-US.resw"))
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        en.entries.iter().any(|e| e.comment.is_some()),
        "resw comments"
    );
    assert!(
        resx::values(&en)
            .values()
            .any(|v| v.contains('&') || v.contains('<') || v.contains('\'')),
        "entities decode"
    );
}

#[test]
fn resx_edit_insert_and_new_file() {
    let src = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  <resheader name=\"resmimetype\">\n    <value>text/microsoft-resx</value>\n  </resheader>\n  <data name=\"Save\" xml:space=\"preserve\">\n    <value>Save &amp; close</value>\n    <comment>Button</comment>\n  </data>\n  <data name=\"Icon\" type=\"System.Drawing.Bitmap\" mimetype=\"x\">\n    <value>AAAA</value>\n  </data>\n  <data name=\"Empty\" xml:space=\"preserve\">\n    <value />\n  </data>\n</root>\n";
    let mut doc = resx::parse(src).unwrap();
    assert_eq!(resx::serialize(&doc), src);
    let units = resx::units(&doc);
    assert_eq!(
        units.iter().map(|u| u.key.as_str()).collect::<Vec<_>>(),
        ["Save", "Empty"]
    );
    assert_eq!(units[0].source, "Save & close");
    assert_eq!(units[0].comment.as_deref(), Some("Button"));

    doc.set_text(doc.index_of("Save").unwrap(), "Speichern & schließen");
    let out = resx::serialize(&doc);
    assert!(
        out.contains("<value>Speichern &amp; schließen</value>"),
        "{out}"
    );
    doc.insert("Close", "Schließen <b>");
    let out = resx::serialize(&doc);
    assert!(out.contains("  <data name=\"Close\" xml:space=\"preserve\">\n    <value>Schließen &lt;b&gt;</value>\n  </data>\n</root>\n"), "{out}");
    assert_eq!(
        resx::values(&resx::parse(&out).unwrap())["Close"],
        "Schließen <b>"
    );

    let fresh = resx::new_locale_file(src);
    assert!(
        fresh.contains("<resheader") && !fresh.contains("<data "),
        "{fresh}"
    );
    assert!(resx::parse(&fresh).unwrap().entries.is_empty());
}

fn run(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn po_and_resx_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locale/en/LC_MESSAGES")).unwrap();
    fs::write(
        root.join("locale/en/LC_MESSAGES/app.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\n#: views.py:1\nmsgid \"Save\"\nmsgstr \"\"\n\nmsgid \"Delete %(name)s\"\nmsgstr \"\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("Strings/en-US")).unwrap();
    fs::write(
        root.join("Strings/en-US/Resources.resw"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  <data name=\"Title\" xml:space=\"preserve\">\n    <value>Files</value>\n  </data>\n  <data name=\"Open\" xml:space=\"preserve\">\n    <value>Open {0}</value>\n  </data>\n</root>\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("Strings/de-DE")).unwrap();
    fs::write(
        root.join("Strings/de-DE/Resources.resw"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  <data name=\"Title\" xml:space=\"preserve\">\n    <value>Dateien</value>\n  </data>\n</root>\n",
    )
    .unwrap();
    let out = run(root, &["init"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cfg = fs::read_to_string(root.join("polygo.toml")).unwrap();
    assert!(
        cfg.contains("format = \"po\"") && cfg.contains("format = \"resx\""),
        "{cfg}"
    );
    assert!(cfg.contains("locale/{locale}/LC_MESSAGES/app.po"), "{cfg}");
    assert!(cfg.contains("Strings/{locale}/Resources.resw"), "{cfg}");
    let cfg = cfg.replace("kind = \"ollama\"", "kind = \"mock\"");
    fs::write(root.join("polygo.toml"), cfg).unwrap();
    let out = run(root, &["translate"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de_po = fs::read_to_string(root.join("locale/de-DE/LC_MESSAGES/app.po")).unwrap();
    assert!(de_po.contains("\"Language: de-DE\\n\""), "{de_po}");
    assert!(
        de_po.contains("msgid \"Save\"\nmsgstr \"⟦de-DE⟧ Save\""),
        "{de_po}"
    );
    assert!(
        de_po.contains("msgstr \"⟦de-DE⟧ Delete %(name)s\""),
        "{de_po}"
    );
    let de_resw = fs::read_to_string(root.join("Strings/de-DE/Resources.resw")).unwrap();
    assert!(
        de_resw.contains("<value>Dateien</value>"),
        "human kept:\n{de_resw}"
    );
    assert!(
        de_resw.contains("<value>⟦de-DE⟧ Open {0}</value>"),
        "{de_resw}"
    );
    assert_eq!(run(root, &["check"]).status.code(), Some(0));
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
}
