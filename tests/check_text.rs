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
    // Acronyms and tiny tokens are the same everywhere.
    assert!(kinds("OK", "OK", "de", 2.5).is_empty());
    assert!(kinds("URL", "URL", "ja", 2.5).is_empty());
    assert!(kinds("PDF export", "PDF export", "de", 2.5).len() == 1);
    assert!(kinds("%d", "%d", "de", 2.5).is_empty());
    assert!(kinds("2024", "2024", "ja", 2.5).is_empty());
    // Untranslated markup-only strings are fine.
    assert!(kinds("<br/>", "<br/>", "fr", 2.5).is_empty());
}

#[test]
fn fragment_flags_source_words_left_in_non_latin_translations() {
    use polygo::check::text::untranslated_fragment;
    let f = |s: &str, t: &str| untranslated_fragment(s, t, &[]);
    assert_eq!(
        f("Welcome back, %@!", "%@さん、ようこそ back!"),
        Some(vec!["back".to_string()])
    );
    assert_eq!(
        f("Delete %lld photos?", "Удалить %lld photos?"),
        Some(vec!["photos".to_string()])
    );
    // Clean translations, loanwords, acronyms, brand names and mixed case pass.
    assert_eq!(f("Welcome back, %@!", "おかえりなさい、%@さん！"), None);
    assert_eq!(f("Send email", "Отправить email"), None);
    assert_eq!(f("Open in iOS", "iOSで開く"), None);
    assert_eq!(f("Sign in with GitHub", "GitHubでサインイン"), None);
    assert_eq!(f("Save as PDF", "PDFとして保存"), None);
    // Latin-script targets are never judged (German keeps "Tab" legitimately).
    assert_eq!(f("Close tab", "Tab schließen"), None);
    assert_eq!(f("Close the tab now", "Close the tab jetzt"), None);
    // Allowed terms (glossary / do-not-translate) are skipped, even multi-word.
    assert_eq!(
        untranslated_fragment(
            "Open Google Drive folder",
            "Google Drive フォルダを開く",
            &[]
        ),
        None
    );
    assert_eq!(
        untranslated_fragment(
            "Use dark mode",
            "dark mode を使用してください",
            &["dark mode".to_string()]
        ),
        None
    );
    assert_eq!(
        untranslated_fragment("Use dark mode", "dark mode を使用してください", &[]),
        Some(vec!["mode".to_string()])
    );
}

#[test]
fn check_text_reports_fragment_as_warning() {
    use polygo::check::text::{Severity, check_text};
    let f = check_text(
        "Welcome back, %@!",
        "%@さん、ようこそ back!",
        "en",
        "ja",
        2.5,
    );
    assert!(
        f.iter()
            .any(|x| x.code == "fragment" && x.severity == Severity::Warning),
        "{f:?}"
    );
}
