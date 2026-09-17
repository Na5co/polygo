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
    assert!(String::from_utf8_lossy(&out.stdout).contains("target_locales = [de, bg, pt-BR]"));
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
