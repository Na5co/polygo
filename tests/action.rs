//! G5.4: the Action's script, dry-run in a fixture git repo, produces the expected diff.
#![cfg(unix)] // runs the shell scripts; the Action itself only runs on ubuntu
use std::fs;
use std::process::Command;

#[test]
fn action_dry_run_produces_expected_diff() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"hello\": \"Hello\",\n  \"bye\": \"Goodbye\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"hello\": \"Hallo\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"fr\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "-A"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-q",
        "-m",
        "init",
    ]);

    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/action/run.sh");
    let out = Command::new("bash")
        .current_dir(root)
        .arg(script)
        .env("POLYGO_BIN", env!("CARGO_BIN_EXE_polygo"))
        // The developer's own translation memory must not answer for the mock.
        .env("POLYGO_CONFIG_DIR", root.join("cfg"))
        .env("DRY_RUN", "1")
        .env("GITHUB_OUTPUT", root.join("gh_output.txt"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The diff shows the new German key and the new French file; nothing was committed.
    assert!(stdout.contains("+  \"bye\": \"⟦de⟧ Goodbye\""), "{stdout}");
    assert!(stdout.contains("dry run: not committing"), "{stdout}");
    assert!(
        fs::read_to_string(root.join("gh_output.txt"))
            .unwrap()
            .contains("changed=true")
    );
    let log = Command::new("git")
        .current_dir(root)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).lines().count(),
        1,
        "dry run must not commit"
    );
    assert!(root.join("locales/fr.json").exists());
    assert!(root.join("polygo.lock").exists());

    // Commit mode commits exactly the translation changes.
    let out = Command::new("bash")
        .current_dir(root)
        .arg(script)
        .env("POLYGO_BIN", env!("CARGO_BIN_EXE_polygo"))
        .env("DRY_RUN", "0")
        .env("GITHUB_OUTPUT", root.join("gh_output.txt"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let log = Command::new("git")
        .current_dir(root)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert_eq!(log.lines().count(), 2, "{log}");
    assert!(
        log.contains("chore(i18n): update translations with polygo"),
        "{log}"
    );
    // Third run: nothing to do.
    let out = Command::new("bash")
        .current_dir(root)
        .arg(script)
        .env("POLYGO_BIN", env!("CARGO_BIN_EXE_polygo"))
        .env("GITHUB_OUTPUT", root.join("gh_output.txt"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("no translation changes"));
}

#[test]
fn action_check_mode_annotates_and_fails_on_errors() {
    // No polygo.toml: the check mode points at a directory and everything is detected.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"greet\": \"Hi {{name}}\",\n  \"ok\": \"Settings\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{nome}}\",\n  \"ok\": \"Settings\"\n}\n",
    )
    .unwrap();
    let summary = root.join("summary.md");
    let outputs = root.join("outputs.txt");
    let run = |strict: bool| {
        Command::new("bash")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/action/check.sh"))
            .current_dir(root)
            .env("POLYGO_BIN", env!("CARGO_BIN_EXE_polygo"))
            .env("POLYGO_PATH", "locales")
            .env("POLYGO_STRICT", if strict { "1" } else { "0" })
            .env("GITHUB_STEP_SUMMARY", &summary)
            .env("GITHUB_OUTPUT", &outputs)
            .output()
            .unwrap()
    };
    let out = run(false);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // One annotation per finding, on the file, with key and locale in the title.
    assert!(
        stdout.contains(
            "::error file=locales/de.json,line=2,title=polygo placeholders [de]::greet: "
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("::warning file=locales/de.json,line=3,title=polygo identical [de]::ok: "),
        "{stdout}"
    );
    assert!(
        stdout.contains("polygo check: 1 error(s), 1 warning(s), 2 translation(s) checked"),
        "{stdout}"
    );
    let md = fs::read_to_string(&summary).unwrap();
    assert!(md.contains("## polygo check"), "{md}");
    assert!(
        md.contains("**1 error(s), 1 warning(s)** in 2 translation(s)."),
        "{md}"
    );
    assert!(
        md.contains("| ❌ | `locales/de.json:2` | `greet` | de | placeholders: "),
        "{md}"
    );
    let o = fs::read_to_string(&outputs).unwrap();
    assert!(
        o.contains("errors=1\n") && o.contains("warnings=1\n") && o.contains("checked=2\n"),
        "{o}"
    );

    // Strict: the warning is reported as an error annotation and the totals still say warning.
    let out = run(true);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("::error file=locales/de.json,line=3,title=polygo identical [de]")
    );

    // Fixed: exit 0, green summary.
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"greet\": \"Hallo {{name}}\",\n  \"ok\": \"Einstellungen\"\n}\n",
    )
    .unwrap();
    fs::remove_file(&summary).unwrap();
    let out = run(false);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        fs::read_to_string(&summary)
            .unwrap()
            .contains("✅ 2 translation(s) checked, no problems.")
    );
}

#[test]
fn action_yml_has_a_check_mode() {
    let yml =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/action/action.yml")).unwrap();
    for needle in [
        "mode:",
        "path:",
        "strict:",
        "sarif:",
        "upload-sarif",
        "check.sh",
        "inputs.mode == 'check'",
        "errors:",
        "warnings:",
    ] {
        assert!(yml.contains(needle), "action.yml lacks {needle}");
    }
}
