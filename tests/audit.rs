//! `polygo audit`: a judge model scores translations; flagged ones are listed with a
//! reason and exit 1; `--fix` re-translates them but never touches human edits.
use std::fs;
use std::process::Command;

fn polygo(root: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_polygo"));
    c.current_dir(root)
        .env("POLYGO_NO_BACKOFF", "1")
        .env("POLYGO_CONFIG_DIR", root.join("cfg"));
    c
}

#[test]
fn audit_flags_bad_translations_and_fix_respects_human_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"k1\": \"Save\",\n  \"k2\": \"Accessibility access\",\n  \"k3\": \"Open\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"bg\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    // Translate with the mock, then hand-edit k3 (a human correction).
    assert!(
        polygo(root)
            .arg("translate")
            .output()
            .unwrap()
            .status
            .success()
    );
    let bg = fs::read_to_string(root.join("locales/bg.json")).unwrap();
    fs::write(
        root.join("locales/bg.json"),
        bg.replace("⟦bg⟧ Open", "Отвори"),
    )
    .unwrap();

    // Judge (mock) scores k2 and k3 as wrong.
    let out = polygo(root)
        .env("POLYGO_MOCK_AUDIT_BAD", "k2,k3")
        .args(["audit", "--judge", "mock/judge"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("1/5  [bg]  k2") && text.contains("wrong meaning (mock)"),
        "{text}"
    );
    assert!(text.contains("k3") && !text.contains("k1"), "{text}");
    assert!(text.contains("2 translation(s) scored ≤ 3"), "{text}");
    let out = polygo(root)
        .env("POLYGO_MOCK_AUDIT_BAD", "k2")
        .args(["audit", "--judge", "mock/judge", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["key"], "k2");
    assert_eq!(v[0]["score"], 1);

    // Nothing flagged → exit 0.
    let out = polygo(root)
        .args(["audit", "--judge", "mock/judge"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing scored"));

    // --fix rewrites k2 with the judge model, leaves the human-edited k3 alone.
    let out = polygo(root)
        .env("POLYGO_MOCK_AUDIT_BAD", "k2,k3")
        .args(["audit", "--judge", "mock/judge", "--fix"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("1 flagged string(s) are human-edited"),
        "{err}"
    );
    let bg = fs::read_to_string(root.join("locales/bg.json")).unwrap();
    assert!(bg.contains("\"k3\": \"Отвори\""), "{bg}");
    assert!(bg.contains("\"k2\": \"⟦bg⟧ Accessibility access\""), "{bg}");
    let lock = fs::read_to_string(root.join("polygo.lock")).unwrap();
    assert!(lock.contains("[keys.k2.locales.bg]"), "{lock}");
}
