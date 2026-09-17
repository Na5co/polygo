//! `polygo status --markdown`: a coverage table for READMEs and PR comments.
use std::fs;
use std::process::Command;

#[test]
fn status_markdown_table_has_coverage_per_locale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"a\": \"A\",\n  \"b\": \"B\",\n  \"c\": \"C\",\n  \"d\": \"D\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"a\": \"A!\",\n  \"b\": \"B!\",\n  \"c\": \"C!\"\n}\n",
    )
    .unwrap();
    fs::write(root.join("locales/ja.json"), "{\n  \"a\": \"あ\"\n}\n").unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"ja\", \"pt-BR\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["status", "--markdown"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let md = String::from_utf8(out.stdout).unwrap();
    assert!(md.contains("| Locale | Translated | Coverage |"), "{md}");
    assert!(md.contains("| 🇩🇪 de | 3 / 4 | 75% |"), "{md}");
    assert!(md.contains("| 🇯🇵 ja | 1 / 4 | 25% |"), "{md}");
    assert!(md.contains("| 🇧🇷 pt-BR | 0 / 4 | 0% |"), "{md}");
    assert!(md.contains("4 strings · 3 locales"), "{md}");
}
