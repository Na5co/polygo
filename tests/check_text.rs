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
        [
            ("punctuation".to_string(), Severity::Warning),
            ("length".to_string(), Severity::Warning)
        ]
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
    // A key standing in for the source (Penpot keeps English in en.po) has no length to
    // compare with; an abbreviation is as long as the language's word for it.
    assert!(kinds("dashboard.settings.notifications.none", "Keine", "de", 2.5).is_empty());
    assert!(kinds("CVV", "Cryptogramme visuel", "fr", 2.5).is_empty());
    assert!(kinds("AI", "الذكاء الاصطناعي", "ar", 2.5).is_empty());

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
            .any(|x| x.code.as_str() == "fragment" && x.severity == Severity::Warning),
        "{f:?}"
    );
}

#[test]
fn fragment_ignores_entities_handles_paths_and_hyphenated_code() {
    use polygo::check::text::untranslated_fragment;
    let f = |s: &str, t: &str| untranslated_fragment(s, t, &[]);
    assert_eq!(
        f(
            "Still open &mdash; {{0}}.",
            "Все още отворено &mdash; {{0}}."
        ),
        None
    );
    assert_eq!(
        f(
            "Slack &middot; used by @slack",
            "Slack &middot; използва се от @slack"
        ),
        None
    );
    assert_eq!(
        f(
            "Nothing, only {{0}}/form-name works",
            "Нищо, само {{0}}/form-name работи"
        ),
        None
    );
    assert_eq!(
        f(
            "Mail user@example.com now",
            "Пишете на user@example.com сега"
        ),
        None
    );
    // A real stray word still fires.
    assert_eq!(
        f(
            "Still open &mdash; check later.",
            "Все още отворено &mdash; check по-късно."
        ),
        Some(vec!["check".to_string()])
    );
}

#[test]
fn markup_whitespace_and_punctuation() {
    use polygo::check::text::{edge_whitespace_mismatch, markup_mismatch, punctuation_dropped};
    // Markup: a dropped closing tag, an added tag, attributes ignored, `a < b` not a tag.
    assert_eq!(
        markup_mismatch("Press <b>Save</b>", "Drücke <b>Speichern"),
        Some("missing </b>".into())
    );
    assert_eq!(
        markup_mismatch(
            "See <a href=\"x\">docs</a>",
            "Siehe <a href=\"y\" target=\"_blank\">Doku</a>"
        ),
        None
    );
    assert_eq!(
        markup_mismatch("Plain", "<i>Kursiv</i>"),
        Some("unexpected </i>, <i>".into())
    );
    assert_eq!(markup_mismatch("a < b and c > d", "a < b und c > d"), None);
    assert_eq!(markup_mismatch("Line<br/>break", "Zeilen<br>umbruch"), None);
    assert_eq!(
        kinds("Press <b>Save</b>", "Drücke <b>Speichern", "de", 2.5),
        [("markup".to_string(), Severity::Error)]
    );

    // Edges: a trailing space or newline that vanished, or appeared.
    assert_eq!(
        edge_whitespace_mismatch("Name: ", "Name:"),
        Some("trailing \"\" vs \"␠\" in the source".into())
    );
    assert_eq!(
        edge_whitespace_mismatch("Done\n", "Fertig"),
        Some("trailing \"\" vs \"\\n\" in the source".into())
    );
    assert_eq!(
        edge_whitespace_mismatch("Done", " Fertig"),
        Some("leading \"␠\" vs \"\" in the source".into())
    );
    assert_eq!(edge_whitespace_mismatch(" Both ", " Beide "), None);
    assert_eq!(edge_whitespace_mismatch("Done", "Fertig"), None);

    // Punctuation: dropped ellipsis/colon/full stop; other scripts' marks and quotes pass;
    // short labels and placeholder endings are ignored.
    assert!(punctuation_dropped("Settings…", "Ajustes").is_some());
    assert!(punctuation_dropped("Enter your password:", "Passwort eingeben").is_some());
    assert!(
        punctuation_dropped(
            "This cannot be undone.",
            "Das kann nicht rückgängig gemacht werden"
        )
        .is_some()
    );
    assert!(punctuation_dropped("This cannot be undone.", "これは元に戻せません。").is_none());
    assert!(punctuation_dropped("Are you sure?", "هل أنت متأكد؟").is_none());
    assert!(punctuation_dropped("Delete it?", "Löschen?").is_none());
    assert!(punctuation_dropped("OK.", "OK").is_none());
    assert!(punctuation_dropped("Total: %d", "Gesamt: %d").is_none());
    assert!(punctuation_dropped("Save changes", "Änderungen speichern").is_none());
    assert!(punctuation_dropped("He said \"go.\"", "Er sagte „geh.“").is_none());
}
