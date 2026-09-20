//! The release workflow, the GitHub Action, install.sh and the Homebrew formula
//! generator agree on asset names, and the size budget is enforced where binaries are built.
use std::fs;

fn read(rel: &str) -> String {
    fs::read_to_string(format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn release_assets_match_every_consumer() {
    let release = read(".github/workflows/release.yml");
    let action = read("action/action.yml");
    let install = read("install.sh");
    let formula = read("scripts/formula.sh");
    // Consumers download polygo-<tag>-<arch>-<os>.tar.gz.
    assert!(
        action.contains("polygo-${v}-${arch}-${os}.tar.gz"),
        "{action}"
    );
    assert!(
        install.contains("polygo-$tag-$arch-$os.tar.gz"),
        "{install}"
    );
    assert!(formula.contains("polygo-v$v-$1.tar.gz"), "{formula}");
    for asset in [
        "aarch64-darwin",
        "x86_64-darwin",
        "x86_64-linux",
        "aarch64-linux",
        "x86_64-windows",
    ] {
        assert!(
            release.contains(&format!("asset: {asset}")),
            "release.yml lacks {asset}"
        );
    }
    assert!(release.contains("polygo-${v}-${{ matrix.asset }}.tar.gz"));
    assert!(release.contains("polygo-${v}-${{ matrix.asset }}.zip"));
    for a in [
        "aarch64-darwin",
        "x86_64-darwin",
        "aarch64-linux",
        "x86_64-linux",
    ] {
        assert!(formula.contains(a), "formula.sh lacks {a}");
    }
    // Size budget wherever a release binary is produced.
    assert!(release.contains("15728640"));
    assert!(read(".github/workflows/ci.yml").contains("15728640"));
    assert!(read("scripts/release.sh").contains("15728640"));
    assert!(read(".github/workflows/ci.yml").contains("cargo clippy --all-targets -- -D warnings"));
    // The formula is generated, not checked in.
    assert!(!std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/packaging")).exists());
}

#[test]
fn formula_generator_fills_version_and_checksums() {
    let dir = tempfile::tempdir().unwrap();
    let sums = dir.path().join("SHA256SUMS");
    let mut text = String::new();
    for (i, a) in [
        "aarch64-darwin",
        "x86_64-darwin",
        "aarch64-linux",
        "x86_64-linux",
    ]
    .iter()
    .enumerate()
    {
        text.push_str(&format!(
            "{}  polygo-v9.9.9-{a}.tar.gz\n",
            format!("{i}").repeat(64)
        ));
    }
    text.push_str("ffff  polygo-v9.9.9-x86_64-windows.zip\n");
    fs::write(&sums, text).unwrap();
    let out = std::process::Command::new("sh")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/formula.sh"))
        .arg("9.9.9")
        .arg(&sums)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rb = String::from_utf8_lossy(&out.stdout);
    assert!(rb.contains("version \"9.9.9\""), "{rb}");
    assert!(
        rb.contains(&format!("sha256 \"{}\"", "0".repeat(64))),
        "{rb}"
    );
    assert!(
        rb.contains(&format!("sha256 \"{}\"", "3".repeat(64))),
        "{rb}"
    );
    assert!(rb.contains("generate_completions_from_executable"), "{rb}");
    // A missing asset is an error, not a formula with an empty sha256.
    fs::write(&sums, "aaaa  polygo-v9.9.9-aarch64-darwin.tar.gz\n").unwrap();
    let out = std::process::Command::new("sh")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/formula.sh"))
        .arg("9.9.9")
        .arg(&sums)
        .output()
        .unwrap();
    assert!(!out.status.success());
}
