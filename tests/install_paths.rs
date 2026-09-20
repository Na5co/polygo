//! G6.2: install artifacts are consistent with the release layout and syntactically valid.
#![cfg(unix)] // runs the shell scripts; the Action itself only runs on ubuntu
use std::fs;
use std::process::Command;

#[test]
fn install_scripts_are_valid_and_consistent() {
    let root = env!("CARGO_MANIFEST_DIR");
    // install.sh is POSIX sh and parses.
    let out = Command::new("sh")
        .args(["-n", &format!("{root}/install.sh")])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sh = fs::read_to_string(format!("{root}/install.sh")).unwrap();
    assert!(sh.contains("polygo-$tag-$arch-$os.tar.gz"));
    // The generated Homebrew formula covers the four Unix assets with the same naming.
    let rb = fs::read_to_string(format!("{root}/scripts/formula.sh")).unwrap();
    for asset in [
        "aarch64-darwin",
        "x86_64-darwin",
        "aarch64-linux",
        "x86_64-linux",
    ] {
        assert!(
            rb.contains(&format!("polygo-v#{{version}}-{asset}.tar.gz")),
            "formula lacks {asset}"
        );
    }
    // cargo-binstall metadata points at the same layout.
    let toml = fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    assert!(toml.contains("[package.metadata.binstall]"));
    assert!(
        toml.contains("polygo-v{ version }-{ target-arch }-{ target-family }{ archive-suffix }")
    );
}
