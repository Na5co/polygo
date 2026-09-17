//! G6.4: a docs page per format, `--help` examples on every subcommand, a CHANGELOG.
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn docs_page_per_format_covers_layout_plurals_and_placeholders() {
    for (name, layout_hint) in [
        ("xcstrings", ".xcstrings"),
        ("android", "values-{android_locale}"),
        ("json", "{locale}"),
        ("arb", "{locale}"),
        ("po", "LC_MESSAGES"),
        ("resx", "{locale}"),
    ] {
        let path = root().join(format!("docs/formats/{name}.md"));
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for needle in [
            "polygo init",
            "[[files]]",
            layout_hint,
            "## Plurals",
            "## Placeholders",
            "## What is preserved",
        ] {
            assert!(text.contains(needle), "{name}.md lacks `{needle}`");
        }
        assert!(
            text.lines().next().unwrap().starts_with("# "),
            "{name}.md title"
        );
    }
    let index = fs::read_to_string(root().join("docs/README.md")).unwrap();
    for name in ["xcstrings", "android", "json", "arb", "po", "resx"] {
        assert!(
            index.contains(&format!("formats/{name}.md")),
            "index lacks {name}"
        );
    }
}

#[test]
fn help_has_examples_for_every_subcommand() {
    for sub in ["", "init", "translate", "check", "status", "review"] {
        let mut args: Vec<&str> = vec![];
        if !sub.is_empty() {
            args.push(sub);
        }
        args.push("--help");
        let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
            .args(&args)
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(out.status.success());
        assert!(
            text.contains("Examples:"),
            "`polygo {sub} --help` has no Examples:\n{text}"
        );
        assert!(
            text.contains("  polygo "),
            "{sub}: examples must be indented `polygo …` lines"
        );
    }
}

#[test]
fn changelog_matches_cargo_version() {
    let log = fs::read_to_string(root().join("CHANGELOG.md")).unwrap();
    assert!(log.starts_with("# Changelog"));
    assert!(
        log.contains(&format!("## {}", env!("CARGO_PKG_VERSION"))),
        "CHANGELOG has no section for {}",
        env!("CARGO_PKG_VERSION")
    );
    for fmt in ["xcstrings", "Android", "i18next", "ARB", "gettext", "resx"] {
        assert!(log.contains(fmt), "CHANGELOG does not mention {fmt}");
    }
}
