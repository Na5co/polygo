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
