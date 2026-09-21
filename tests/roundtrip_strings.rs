//! Legacy Apple `.strings`: detection, check, translate and byte-stable write-back,
//! including a UTF-16 locale file that must stay UTF-16.
use std::fs;
use std::path::Path;
use std::process::Command;

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(args)
        .env("POLYGO_CONFIG_DIR", root.join("cfg"))
        .env("POLYGO_NO_BACKOFF", "1")
        .output()
        .unwrap()
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    out
}

#[test]
fn strings_files_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let app = root.join("App");
    fs::create_dir_all(app.join("en.lproj")).unwrap();
    fs::create_dir_all(app.join("de.lproj")).unwrap();
    fs::create_dir_all(app.join("fr.lproj")).unwrap();
    fs::write(
        app.join("en.lproj/Localizable.strings"),
        "/* Greeting on the home screen */\n\"welcome\" = \"Welcome, %@!\";\n\n/* Toolbar */\n\"open\" = \"Open\";\n\"count\" = \"%d items\";\n",
    )
    .unwrap();
    // German: UTF-16, a human translation with the placeholder dropped, one key missing.
    fs::write(
        app.join("de.lproj/Localizable.strings"),
        utf16le("/* Greeting on the home screen */\n\"welcome\" = \"Willkommen!\";\n\n\"open\" = \"Öffnen\";\n"),
    )
    .unwrap();
    fs::write(app.join("fr.lproj/Localizable.strings"), "").unwrap();
    // Also an InfoPlist.strings, its own spec.
    fs::write(
        app.join("en.lproj/InfoPlist.strings"),
        "CFBundleDisplayName = \"My App\";\n",
    )
    .unwrap();

    // Zero-config check by directory: format + locales detected; UTF-16 read fine.
    let out = run(root, &["check", "App"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("error   App/de.lproj/Localizable.strings:2  en.lproj/Localizable.strings:welcome  [de]  placeholders: missing %1$@"),
        "{text}"
    );
    assert!(text.contains("de 50% (2 of 4 missing)"), "{text}");

    // init detects two specs (Localizable + InfoPlist) and both target locales.
    let out = run(root, &["init"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cfg = fs::read_to_string(root.join("polygo.toml")).unwrap();
    assert!(cfg.contains("format = \"strings\""), "{cfg}");
    assert!(
        cfg.contains("path = \"App/en.lproj/Localizable.strings\""),
        "{cfg}"
    );
    assert!(
        cfg.contains("locale_path = \"App/{locale}.lproj/Localizable.strings\""),
        "{cfg}"
    );
    assert!(
        cfg.contains("path = \"App/en.lproj/InfoPlist.strings\""),
        "{cfg}"
    );
    assert!(cfg.contains("target_locales = [\"de\", \"fr\"]"), "{cfg}");
    fs::write(
        root.join("polygo.toml"),
        cfg.replace("kind = \"ollama\"", "kind = \"mock\""),
    )
    .unwrap();

    let out = run(root, &["translate"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // German stays UTF-16 with its BOM; the human line is untouched; the missing key is
    // appended with the source comment.
    let de = fs::read(app.join("de.lproj/Localizable.strings")).unwrap();
    assert_eq!(&de[..2], &[0xFF, 0xFE]);
    let (de_text, enc) = polygo::formats::strings::decode_file(&de).unwrap();
    assert_eq!(enc, polygo::formats::strings::Encoding::Utf16Le);
    assert_eq!(
        de_text,
        "/* Greeting on the home screen */\n\"welcome\" = \"Willkommen!\";\n\n\"open\" = \"Öffnen\";\n\"count\" = \"⟦de⟧ %d items\";\n"
    );
    // French was empty: everything appended in source order, comments carried over.
    let fr = fs::read_to_string(app.join("fr.lproj/Localizable.strings")).unwrap();
    assert_eq!(
        fr,
        "/* Greeting on the home screen */\n\"welcome\" = \"⟦fr⟧ Welcome, %@!\";\n/* Toolbar */\n\"open\" = \"⟦fr⟧ Open\";\n\"count\" = \"⟦fr⟧ %d items\";\n"
    );
    assert!(app.join("fr.lproj/InfoPlist.strings").exists());
    // Byte-stable: parse + serialize gives back the same text.
    for f in [
        "en.lproj/Localizable.strings",
        "fr.lproj/Localizable.strings",
    ] {
        let t = fs::read_to_string(app.join(f)).unwrap();
        let doc = polygo::formats::strings::parse(&t).unwrap();
        assert_eq!(polygo::formats::strings::serialize(&doc), t, "{f}");
    }
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
    let out = run(root, &["status"]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("de       new: 0  stale: 0  untranslated: 0  edited: 2  up-to-date: 2"),
        "{s}"
    );
}
