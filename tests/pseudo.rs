//! `polygo pseudo`: accented, longer, bracketed; placeholders and markup untouched.
use polygo::pseudo::transform;
use std::fs;
use std::process::Command;

#[test]
fn transform_keeps_every_placeholder_kind() {
    for src in [
        "Delete %lld photos from %@?",
        "Hello {{name}}, you have {count, plural, one {# item} other {# items}}",
        "Open %1$s in <b>%2$s</b> &amp; save",
        "See $t(other.key) and $name",
        "100%% done \\n next",
        "%#@files@ selected",
    ] {
        let out = transform(src);
        assert!(out.starts_with('[') && out.ends_with(']'), "{out}");
        for p in polygo::check::placeholders::extract(src) {
            let _ = p; // just ensure the checker sees no difference:
        }
        assert!(
            polygo::check::placeholders::compare(src, &out).is_none(),
            "{src} → {out}"
        );
        assert!(
            out.chars().count() > src.chars().count() + 2,
            "{src} → {out}"
        );
        assert!(!out.contains("Delete") && !out.contains("Hello"), "{out}");
    }
    assert_eq!(transform("Save"), "[Šáṽé ~~]");
    assert_eq!(transform("%@"), "[%@]");
}

#[test]
fn pseudo_writes_a_locale_for_xcstrings_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"hi\": \"Hello {{name}}\",\n  \"n\": {\n    \"one\": \"{{count}} item\",\n    \"other\": \"{{count}} items\"\n  }\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("pseudo")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let xa = fs::read_to_string(root.join("locales/en-XA.json")).unwrap();
    assert!(xa.contains("\"hi\": \"[Ĥéļļö {{name}} ~~]\""), "{xa}");
    assert!(xa.contains("\"other\": \"[{{count}} íţéɱš ~~]\""), "{xa}");
    assert!(
        !root.join("polygo.lock").exists()
            || !fs::read_to_string(root.join("polygo.lock"))
                .unwrap()
                .contains("en-XA")
    );
    // Real targets untouched.
    assert!(!root.join("locales/de.json").exists());
}
