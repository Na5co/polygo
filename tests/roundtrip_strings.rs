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

#[test]
fn stringsdict_plurals_are_checked_per_locale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let app = root.join("App");
    for l in ["en", "de", "pl", "ru"] {
        fs::create_dir_all(app.join(format!("{l}.lproj"))).unwrap();
        fs::write(
            app.join(format!("{l}.lproj/Localizable.strings")),
            "\"ok\" = \"OK\";\n",
        )
        .unwrap();
    }
    let dict = |forms: &str, total_forms: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
	<key>%d files</key>
	<dict>
		<key>NSStringLocalizedFormatKey</key>
		<string>%#@files@</string>
		<key>files</key>
		<dict>
			<key>NSStringFormatSpecTypeKey</key>
			<string>NSStringPluralRuleType</string>
			<key>NSStringFormatValueTypeKey</key>
			<string>d</string>
{forms}
		</dict>
	</dict>
	<key>results</key>
	<dict>
		<key>NSStringLocalizedFormatKey</key>
		<string>%1$#@position@</string>
		<key>position</key>
		<dict>
			<key>NSStringFormatSpecTypeKey</key>
			<string>NSStringPluralRuleType</string>
			<key>NSStringFormatValueTypeKey</key>
			<string>d</string>
			<key>other</key>
			<string>%d of %2$#@total@</string>
		</dict>
		<key>total</key>
		<dict>
			<key>NSStringFormatSpecTypeKey</key>
			<string>NSStringPluralRuleType</string>
			<key>NSStringFormatValueTypeKey</key>
			<string>d</string>
{total_forms}
		</dict>
	</dict>
</dict>
</plist>
"#
        )
    };
    let f = |cat: &str, s: &str| format!("\t\t\t<key>{cat}</key>\n\t\t\t<string>{s}</string>");
    fs::write(
        app.join("en.lproj/PluralAware.stringsdict"),
        dict(
            &[f("one", "%d file"), f("other", "%d files")].join("\n"),
            &[f("one", "%d match"), f("other", "%d matches")].join("\n"),
        ),
    )
    .unwrap();
    // German: `one` without the number is fine; `total` uses its own position %2$d.
    fs::write(
        app.join("de.lproj/PluralAware.stringsdict"),
        dict(
            &[f("one", "Eine Datei"), f("other", "%d Dateien")].join("\n"),
            &[f("one", "%2$d Treffer"), f("other", "%2$d Treffer")].join("\n"),
        ),
    )
    .unwrap();
    // Polish: few/many missing.
    fs::write(
        app.join("pl.lproj/PluralAware.stringsdict"),
        dict(
            &[f("one", "%d plik"), f("other", "%d plików")].join("\n"),
            &[
                f("one", "%d wynik"),
                f("few", "%d wyniki"),
                f("many", "%d wyników"),
                f("other", "%d wyniku"),
            ]
            .join("\n"),
        ),
    )
    .unwrap();
    // Russian: `one` also covers 21, 31: leaving the number out is a bug.
    fs::write(
        app.join("ru.lproj/PluralAware.stringsdict"),
        dict(
            &[
                f("one", "Один файл"),
                f("few", "%d файла"),
                f("many", "%d файлов"),
                f("other", "%d файла"),
            ]
            .join("\n"),
            &[
                f("one", "%d совпадение"),
                f("few", "%d совпадения"),
                f("many", "%d совпадений"),
                f("other", "%d совпадения"),
            ]
            .join("\n"),
        ),
    )
    .unwrap();
    let out = run(root, &["check", "App", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let dict_findings: Vec<(String, String, String, String)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["file"].as_str().unwrap().ends_with(".stringsdict"))
        .map(|f| {
            (
                f["locale"].as_str().unwrap().to_string(),
                f["key"].as_str().unwrap().to_string(),
                f["code"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        dict_findings,
        [
            (
                "pl".to_string(),
                "%d files#files".to_string(),
                "plural".to_string(),
                "missing plural form(s) few, many".to_string()
            ),
            (
                "ru".to_string(),
                "%d files#files.one".to_string(),
                "placeholders".to_string(),
                "missing %1$d".to_string()
            ),
        ],
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let pl = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["locale"] == "pl")
        .unwrap();
    assert_eq!(pl["file"], "App/pl.lproj/PluralAware.stringsdict");
    assert_eq!(pl["line"], 4);
}
