//! G6.5 (offline part): launch drafts exist, posts stay under 300 words, nothing posts.
use std::fs;
use std::path::PathBuf;

fn launch(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("launch")
        .join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn body(text: &str) -> &str {
    let marker = text
        .find("**Body:**")
        .map(|i| i + "**Body:**".len())
        .or_else(|| {
            text.find("## First comment")
                .map(|i| i + text[i..].find('\n').unwrap())
        })
        .expect("post has a body marker");
    &text[marker..]
}

#[test]
fn launch_posts_lead_with_their_format_and_stay_short() {
    for (file, format) in [
        ("reddit-iosprogramming.md", ".xcstrings"),
        ("reddit-flutterdev.md", ".arb"),
        ("reddit-androiddev.md", "strings.xml"),
        ("reddit-reactjs.md", "i18next"),
    ] {
        let text = launch(file);
        let title = text.lines().find(|l| l.starts_with("**Title:**")).unwrap();
        assert!(
            title.contains(format),
            "{file}: title must lead with {format}: {title}"
        );
        let words = body(&text).split_whitespace().count();
        assert!(words < 300, "{file}: {words} words");
        assert!(
            text.contains("https://github.com/atanasa/polygo"),
            "{file}: link"
        );
        assert_eq!(
            text.matches("https://").count(),
            1,
            "{file}: exactly one link"
        );
        assert!(
            !text.to_lowercase().contains("star"),
            "{file}: never ask for stars"
        );
        assert!(
            text.trim_end().ends_with('?'),
            "{file}: end with a question"
        );
    }
    let hn = launch("show-hn.md");
    assert!(hn.contains("Show HN:"));
    assert!(body(&hn).split_whitespace().count() < 300);
    for f in ["terminal-trove.md", "awesome-lists.md", "README.md"] {
        assert!(!launch(f).is_empty());
    }
}

#[test]
fn launch_kit_has_no_posting_automation() {
    // Drafts only: nothing under launch/ or scripts/ talks to reddit / HN / GitHub APIs.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in ["launch", "scripts"] {
        for entry in fs::read_dir(root.join(dir)).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                continue;
            }
            let text = fs::read_to_string(&p).unwrap_or_default().to_lowercase();
            for needle in [
                "reddit.com/api",
                "oauth.reddit",
                "news.ycombinator.com/submit",
                "api.github.com/repos",
            ] {
                assert!(!text.contains(needle), "{}: contains {needle}", p.display());
            }
        }
    }
}
