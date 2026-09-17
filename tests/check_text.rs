//! G3.3: length ratio guard and empty / identical-to-source detection.
use polygo::check::text::{Finding, Severity, check_text};

fn kinds(src: &str, tr: &str, locale: &str, ratio: f64) -> Vec<(String, Severity)> {
    check_text(src, tr, "en", locale, ratio)
        .into_iter()
        .map(|f: Finding| (f.code.to_string(), f.severity))
        .collect()
}

#[test]
fn check_length_identity() {
    // Normal translations pass.
    assert!(kinds("Save", "Speichern", "de", 2.5).is_empty());
    assert!(
        kinds(
            "Delete all items in this folder",
            "Alle Elemente in diesem Ordner löschen",
            "de",
            2.5
        )
        .is_empty()
    );

    // Short strings get absolute slack: "OK" → a 7-char word is fine.
    assert!(kinds("OK", "Akkoord", "nl", 2.5).is_empty());

    // Runaway length is a warning, and the ratio is configurable.
    let long = "Diese Übersetzung ist viel zu lang und enthält offensichtlich eine Erklärung, die niemand wollte, plus noch mehr Text.";
    assert_eq!(
        kinds("Save changes", long, "de", 2.5),
        [("length".to_string(), Severity::Warning)]
    );
    assert!(kinds("Save changes", long, "de", 20.0).is_empty());
    // Suspiciously short for a long source, too.
    assert_eq!(
        kinds(
            "This will permanently delete the selected files and cannot be undone.",
            "Ja",
            "de",
            2.5
        ),
        [("length".to_string(), Severity::Warning)]
    );

    // Empty translation is an error.
    assert_eq!(
        kinds("Save", "", "de", 2.5),
        [("empty".to_string(), Severity::Error)]
    );
    assert_eq!(
        kinds("Save", "   ", "de", 2.5),
        [("empty".to_string(), Severity::Error)]
    );
    // Empty source with empty translation is fine.
    assert!(kinds("", "", "de", 2.5).is_empty());

    // Identical to source: warning when languages differ and the text has letters.
    assert_eq!(
        kinds("Settings", "Settings", "de", 2.5),
        [("identical".to_string(), Severity::Warning)]
    );
    assert!(kinds("Settings", "Settings", "en-GB", 2.5).is_empty());
    assert!(kinds("%d", "%d", "de", 2.5).is_empty());
    assert!(kinds("2024", "2024", "ja", 2.5).is_empty());
    // Untranslated markup-only strings are fine.
    assert!(kinds("<br/>", "<br/>", "fr", 2.5).is_empty());
}
