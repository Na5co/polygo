//! G5.4: the Action's script, dry-run in a fixture git repo, produces the expected diff.
use std::fs;
use std::process::Command;

#[test]
#[cfg(unix)] // runs the shell scripts; the Action itself only runs on ubuntu
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
