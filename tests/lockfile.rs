//! G1.5: lockfile semantics: hashes per key/locale, TOML round-trip, exact staleness.
use polygo::core::Unit;
use polygo::lockfile::{Lock, State};
use std::collections::BTreeMap;

fn unit(key: &str, source: &str, translations: &[(&str, &str)]) -> Unit {
    Unit {
        key: key.to_string(),
        source: source.to_string(),
        comment: None,
        translations: translations
            .iter()
            .map(|(l, t)| (l.to_string(), t.to_string()))
            .collect::<BTreeMap<_, _>>(),
        locales: None,
    }
}

fn project() -> Vec<Unit> {
    vec![
        unit(
            "save",
            "Save",
            &[("de", "Speichern"), ("fr", "Enregistrer")],
        ),
        unit(
            "cancel",
            "Cancel",
            &[("de", "Abbrechen"), ("fr", "Annuler")],
        ),
        unit(
            "hello",
            "Hello, %@!",
            &[("de", "Hallo, %@!"), ("fr", "Bonjour, %@ !")],
        ),
    ]
}

#[test]
fn lockfile_marks_exactly_one_key_stale_in_every_locale() {
    let locales = ["de", "fr"];
    let mut units = project();
    let mut lock = Lock::default();
    lock.record_all(&units, &locales, "mock", "mock-1");

    // Untouched: everything up to date.
    let status = lock.status(&units, &locales);
    for l in locales {
        assert_eq!(status.count(l, State::UpToDate), 3, "{l}");
        assert_eq!(status.count(l, State::Stale), 0, "{l}");
    }

    // Change one source string.
    units[1].source = "Cancel changes".to_string();
    let status = lock.status(&units, &locales);
    for l in locales {
        assert_eq!(
            status.keys(l, State::Stale),
            vec!["cancel".to_string()],
            "{l}"
        );
        assert_eq!(status.count(l, State::UpToDate), 2, "{l}");
        assert_eq!(status.count(l, State::New), 0, "{l}");
    }
}

#[test]
fn lockfile_new_untranslated_and_edited_translation() {
    let locales = ["de", "fr"];
    let mut units = project();
    let mut lock = Lock::default();
    lock.record_all(&units, &locales, "mock", "mock-1");

    // A brand-new key with no translations anywhere.
    units.push(unit("delete", "Delete", &[]));
    // An existing key whose German translation was removed by hand.
    units[0].translations.remove("de");
    // An existing key whose French translation was edited by hand (not via polygo).
    units[2]
        .translations
        .insert("fr".into(), "Salut, %@ !".into());

    let status = lock.status(&units, &locales);
    assert_eq!(status.keys("de", State::New), vec!["delete".to_string()]);
    assert_eq!(status.keys("fr", State::New), vec!["delete".to_string()]);
    assert_eq!(
        status.keys("de", State::Untranslated),
        vec!["save".to_string()]
    );
    assert_eq!(status.keys("fr", State::Untranslated), Vec::<String>::new());
    // Hand edits are respected: the key is reported as edited, never overwritten as stale.
    assert_eq!(status.keys("fr", State::Edited), vec!["hello".to_string()]);
    assert_eq!(status.keys("de", State::Edited), Vec::<String>::new());
}

#[test]
fn lockfile_preexisting_translations_are_never_treated_as_new() {
    // A project that already has human translations and no lockfile yet.
    let units = project();
    let lock = Lock::default();
    let status = lock.status(&units, &["de", "fr", "ja"]);
    assert_eq!(status.count("de", State::Edited), 3);
    assert_eq!(status.count("de", State::New), 0);
    assert_eq!(status.count("ja", State::New), 3);
    assert!(
        status.work("de").is_empty(),
        "must not re-translate human work"
    );
    assert_eq!(status.work("ja").len(), 3);
}

#[test]
fn lockfile_toml_roundtrip_is_stable_and_sorted() {
    let units = project();
    let mut lock = Lock::default();
    lock.record_all(&units, &["fr", "de"], "ollama", "qwen3:8b");
    let text = lock.to_toml();
    let back = Lock::from_toml(&text).unwrap();
    assert_eq!(back, lock);
    assert_eq!(back.to_toml(), text, "serialization must be deterministic");
    // Keys and locales are sorted so diffs are stable regardless of input order.
    let pos = |s: &str| text.find(s).unwrap_or_else(|| panic!("missing {s}"));
    assert!(pos("[keys.cancel]") < pos("[keys.hello]") && pos("[keys.hello]") < pos("[keys.save]"));
    assert!(pos("[keys.cancel.locales.de]") < pos("[keys.cancel.locales.fr]"));
    assert!(text.contains("provider = \"ollama\""));
    assert!(text.contains("model = \"qwen3:8b\""));
}

#[test]
fn lockfile_dropped_keys_are_pruned_on_record() {
    let mut units = project();
    let mut lock = Lock::default();
    lock.record_all(&units, &["de"], "mock", "m");
    units.remove(0);
    lock.record_all(&units, &["de"], "mock", "m");
    assert!(!lock.to_toml().contains("[keys.save]"));
}
