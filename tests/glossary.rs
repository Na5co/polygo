//! G2.3: glossary terms are injected into prompts and enforced on the output.
use polygo::glossary::Glossary;
use polygo::provider::{Ctx, system_prompt};
use std::fs;
use std::process::Command;

fn polygo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
}

const GLOSSARY: &str = "do_not_translate = [\"Polygo\", \"GitHub\"]\n\n[terms.de]\n\"Sign in\" = \"Anmelden\"\n\"workspace\" = \"Arbeitsbereich\"\n";

#[test]
fn glossary_enforced_rules() {
    let g: Glossary = toml::from_str(GLOSSARY).unwrap();
    assert_eq!(
        g.terms_for("de"),
        vec![
            ("Sign in".to_string(), "Anmelden".to_string()),
            ("workspace".to_string(), "Arbeitsbereich".to_string())
        ]
    );
    assert!(g.terms_for("fr").is_empty());
    // Required term present (case-insensitive) → ok.
    assert!(g.satisfied(
        "de",
        "Sign in to your workspace",
        "Bei deinem Arbeitsbereich anmelden"
    ));
    // Required term missing → violation.
    assert!(!g.satisfied("de", "Sign in to continue", "Einloggen, um fortzufahren"));
    // Term not in source → nothing to enforce.
    assert!(g.satisfied("de", "Continue", "Weiter"));
    // do_not_translate must survive verbatim in any locale.
    assert!(g.satisfied("fr", "Open in GitHub", "Ouvrir dans GitHub"));
    assert!(!g.satisfied("fr", "Open in GitHub", "Ouvrir dans Github"));
    assert!(!g.satisfied("ja", "Polygo settings", "ポリゴの設定"));
}

#[test]
fn glossary_enforced_in_prompt() {
    let g: Glossary = toml::from_str(GLOSSARY).unwrap();
    let ctx = Ctx {
        source_locale: "en".into(),
        target_locale: "de".into(),
        glossary: g.terms_for("de"),
        do_not_translate: g.do_not_translate.clone(),
        format_hint: None,
    };
    let p = system_prompt(&ctx);
    assert!(p.contains("Sign in → Anmelden"), "{p}");
    assert!(
        p.contains("Never translate these terms: Polygo, GitHub"),
        "{p}"
    );
}

fn project(root: &std::path::Path, with_glossary: bool) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"login\": \"Sign in to Polygo\",\n  \"ws\": \"Your workspace\",\n  \"plain\": \"Continue\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    if with_glossary {
        fs::write(root.join("glossary.toml"), GLOSSARY).unwrap();
    }
}

#[test]
fn glossary_enforced_end_to_end() {
    // The mock provider honours the glossary it is given, so the output must contain the terms.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), true);
    let out = polygo()
        .current_dir(dir.path())
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(de.contains("Anmelden") && de.contains("Polygo"), "{de}");
    assert!(de.contains("Arbeitsbereich"), "{de}");

    // A provider that ignores the glossary is caught: the key is named and nothing bad is written.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), true);
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_IGNORE_GLOSSARY", "1")
        .arg("translate")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("login") && err.contains("glossary"), "{err}");
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap_or_default();
    assert!(
        !de.contains("\"login\""),
        "violating translation must not be written:\n{de}"
    );
}
