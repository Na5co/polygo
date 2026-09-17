//! Translation memory: human entries are reused across projects without a model call,
//! model entries are only examples, hand edits are learned, review approvals are human.
use std::fs;
use std::process::Command;

fn polygo(root: &std::path::Path, cfg_dir: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_polygo"));
    c.current_dir(root)
        .env("POLYGO_CONFIG_DIR", cfg_dir)
        .env("POLYGO_NO_BACKOFF", "1");
    c
}

fn project(root: &std::path::Path, en: &str, de: Option<&str>) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(root.join("locales/en.json"), en).unwrap();
    if let Some(de) = de {
        fs::write(root.join("locales/de.json"), de).unwrap();
    }
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
}

#[test]
fn human_translations_are_learned_and_reused_across_projects() {
    let cfg = tempfile::tempdir().unwrap();
    // Project A: a human already translated "Save"; "Open" is new.
    let a = tempfile::tempdir().unwrap();
    project(
        a.path(),
        "{\n  \"save\": \"Save\",\n  \"open\": \"Open\"\n}\n",
        Some("{\n  \"save\": \"Speichern\"\n}\n"),
    );
    let out = polygo(a.path(), cfg.path())
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mem = fs::read_to_string(cfg.path().join("memory.toml")).unwrap();
    assert!(
        mem.contains("Speichern") && mem.contains("by = \"human\""),
        "{mem}"
    );
    assert!(
        mem.contains("⟦de⟧ Open"),
        "model output remembered too:\n{mem}"
    );

    // Project B: "Save" comes from memory (human): no model call; "Open" is model-made
    // in memory, so it is NOT reused blindly but translated again.
    let b = tempfile::tempdir().unwrap();
    project(
        b.path(),
        "{\n  \"btn\": \"Save\",\n  \"menu\": \"Open\",\n  \"x\": \"Other\"\n}\n",
        None,
    );
    let out = polygo(b.path(), cfg.path())
        .args(["translate", "-v"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("reused 1 string(s) from translation memory"),
        "{text}"
    );
    let de = fs::read_to_string(b.path().join("locales/de.json")).unwrap();
    assert!(de.contains("\"btn\": \"Speichern\""), "{de}");
    assert!(de.contains("\"menu\": \"⟦de⟧ Open\""), "{de}");
    let lock = fs::read_to_string(b.path().join("polygo.lock")).unwrap();
    assert!(lock.contains("provider = \"memory\""), "{lock}");

    // Status / forget.
    let out = polygo(b.path(), cfg.path()).arg("memory").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("de") && text.contains("1") && text.contains("2"),
        "{text}"
    );
    let out = polygo(b.path(), cfg.path())
        .args(["memory", "--forget"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("forgot 3 entries"));

    // memory = false keeps a project out of it entirely.
    let c = tempfile::tempdir().unwrap();
    project(c.path(), "{\n  \"s\": \"Save\"\n}\n", None);
    let toml = fs::read_to_string(c.path().join("polygo.toml")).unwrap();
    fs::write(
        c.path().join("polygo.toml"),
        format!("memory = false\n{toml}"),
    )
    .unwrap();
    let out = polygo(c.path(), cfg.path())
        .arg("translate")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        !fs::read_to_string(cfg.path().join("memory.toml"))
            .unwrap_or_default()
            .contains("Save")
    );
}
