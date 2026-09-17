//! G6.1: the release workflow and the Action agree on asset names; the size budget is enforced.
use std::fs;

#[test]
fn release_assets_match_action_download_pattern() {
    let release = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/release.yml"
    ))
    .unwrap();
    let action =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/action/action.yml")).unwrap();
    // The Action downloads polygo-<tag>-<arch>-<os>.tar.gz with arch ∈ {x86_64, aarch64}, os ∈ {darwin, linux}.
    assert!(
        action.contains("polygo-${v}-${arch}-${os}.tar.gz"),
        "{action}"
    );
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
    assert!(
        release.contains("15728640"),
        "size budget must be enforced in the release workflow"
    );
    assert!(
        fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.github/workflows/ci.yml"
        ))
        .unwrap()
        .contains("cargo clippy --all-targets -- -D warnings")
    );
}
