use std::process::Command;

fn polygo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
}

#[test]
fn help_lists_every_subcommand() {
    let out = polygo().arg("--help").output().expect("run polygo --help");
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    for cmd in ["translate", "check", "status", "review", "init"] {
        assert!(
            text.contains(cmd),
            "--help is missing subcommand `{cmd}`:\n{text}"
        );
    }
}

#[test]
fn version_prints_crate_version() {
    let out = polygo()
        .arg("--version")
        .output()
        .expect("run polygo --version");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "got: {text}");
}

#[test]
fn subcommand_help_works() {
    for cmd in ["translate", "check", "status", "review", "init"] {
        let out = polygo()
            .args([cmd, "--help"])
            .output()
            .expect("run subcommand help");
        assert!(
            out.status.success(),
            "`{cmd} --help` failed: {:?}",
            out.status
        );
    }
}

#[test]
fn unknown_subcommand_is_an_error() {
    let out = polygo()
        .arg("frobnicate")
        .output()
        .expect("run unknown subcommand");
    assert!(!out.status.success());
}

#[test]
fn add_and_remove_locales() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(root.join("locales/en.json"), "{\n  \"a\": \"A\"\n}\n").unwrap();
    std::fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_polygo"))
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["add", "bg", "pt-BR"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("target_locales = [\"de\", \"bg\", \"pt-BR\"]")
    );
    assert!(!run(&["add", "cli"]).status.success());
    assert!(!run(&["add", "en"]).status.success());
    let out = run(&["remove", "de"]);
    assert!(out.status.success());
    let toml = std::fs::read_to_string(root.join("polygo.toml")).unwrap();
    assert!(
        toml.contains("\"bg\"") && toml.contains("\"pt-BR\"") && !toml.contains("\"de\""),
        "{toml}"
    );
}

#[test]
fn add_and_use_keep_the_users_polygo_toml_intact() {
    // Comments, key order and the one-line array survive an in-place edit.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(root.join("locales/en.json"), "{\n  \"a\": \"A\"\n}\n").unwrap();
    let original = "# our app\nsource_locale = \"en\"\ntarget_locales = [\"de\"]  # marketing wants more\nbatch_size = 5\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\n# local first\nkind = \"mock\"\n";
    std::fs::write(root.join("polygo.toml"), original).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_polygo"))
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["add", "fr", "pt-BR"]).status.success());
    let toml = std::fs::read_to_string(root.join("polygo.toml")).unwrap();
    assert_eq!(
        toml,
        original.replace(
            "target_locales = [\"de\"]",
            "target_locales = [\"de\", \"fr\", \"pt-BR\"]"
        ),
        "{toml}"
    );

    assert!(run(&["use", "mock/fake"]).status.success());
    let toml = std::fs::read_to_string(root.join("polygo.toml")).unwrap();
    assert!(toml.starts_with("# our app\n"), "{toml}");
    assert!(toml.contains("# local first\n"), "{toml}");
    assert!(toml.contains("target_locales = [\"de\", \"fr\", \"pt-BR\"]  # marketing wants more"));
    assert!(toml.contains("batch_size = 5\n"));
    assert!(
        toml.contains("kind = \"mock\"\nmodel = \"fake\"\n"),
        "{toml}"
    );
    assert!(toml.contains("timeout_secs = 300\n"));
    // A `remove` of the last locale says what to do next; `translate` refuses to guess.
    let out = run(&["remove", "de", "fr", "pt-BR", "nl"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("polygo add <locale>"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nl was not a target locale"));
    let out = run(&["translate"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no target locales"), "{err}");
    assert!(err.contains("polygo add"), "{err}");
}

#[test]
fn init_with_no_targets_points_at_add() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(root.join("locales/en.json"), "{\n  \"a\": \"A\"\n}\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("init")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no target locales yet"), "{stdout}");
    assert!(stdout.contains("polygo add"), "{stdout}");
}
