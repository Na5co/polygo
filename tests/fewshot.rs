//! G4.2: pick the most similar existing translations as few-shot examples.
use polygo::context::fewshot::select;

fn pairs() -> Vec<(String, String)> {
    [
        ("Save changes", "Änderungen speichern"),
        ("Discard changes", "Änderungen verwerfen"),
        ("Save", "Speichern"),
        ("Delete account", "Konto löschen"),
        ("Delete %lld files?", "%lld Dateien löschen?"),
        (
            "Are you sure you want to delete this?",
            "Möchtest du das wirklich löschen?",
        ),
        ("The quick brown fox", "Der schnelle braune Fuchs"),
        ("Settings", "Einstellungen"),
    ]
    .iter()
    .map(|(s, t)| (s.to_string(), t.to_string()))
    .collect()
}

#[test]
fn fewshot_selection() {
    let p = pairs();
    // Overlapping content words rank first; stopwords ("the", "you", "to") don't count.
    let ex = select("Delete these changes?", &p, 3);
    let srcs: Vec<&str> = ex.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(srcs.len(), 3, "{srcs:?}");
    assert!(
        srcs.iter().all(|s| {
            let l = s.to_lowercase();
            l.contains("delete") || l.contains("changes")
        }),
        "{srcs:?}"
    );
    assert!(!srcs.contains(&"The quick brown fox") && !srcs.contains(&"Settings"));

    // Exact same source is never its own example.
    let ex = select("Save", &p, 3);
    assert!(ex.iter().all(|(s, _)| s != "Save"), "{ex:?}");
    assert_eq!(ex[0].0, "Save changes");

    // No overlap at all → no examples rather than noise.
    assert!(select("Zebra", &p, 3).is_empty());
    // k caps the result; empty candidate list is fine.
    assert_eq!(select("Delete changes", &p, 1).len(), 1);
    assert!(select("Delete changes", &[], 3).is_empty());
    // Placeholders and punctuation are not tokens.
    let ex = select("%lld items", &p, 3);
    assert!(ex.iter().all(|(s, _)| !s.contains("fox")), "{ex:?}");
}
