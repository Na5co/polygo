//! G1.2: every corpus catalog must survive parse → serialize byte-for-byte.
use std::fs;
use std::path::PathBuf;

fn corpus() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/xcstrings");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "xcstrings"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 10,
        "corpus needs >= 10 files, found {}",
        files.len()
    );
    files
}

#[test]
fn roundtrip_xcstrings() {
    for path in corpus() {
        let original = fs::read_to_string(&path).unwrap();
        let doc = polygo::formats::xcstrings::parse(&original)
            .unwrap_or_else(|e| panic!("{}: parse failed: {e}", path.display()));
        let out = polygo::formats::xcstrings::serialize(&doc);
        if out != original {
            let (line, orig_l, out_l) = first_diff(&original, &out);
            panic!(
                "{}: not byte-stable at line {line}\n  expected: {orig_l:?}\n  actual:   {out_l:?}",
                path.display()
            );
        }
    }
}

#[test]
fn roundtrip_synthetic_edge_cases() {
    // Empty objects, booleans, ints, escapes, no trailing newline.
    let src = "{\n  \"sourceLanguage\" : \"en\",\n  \"strings\" : {\n    \"a\\\"b\\\\c\\nd\" : {\n      \"localizations\" : {\n\n      },\n      \"shouldTranslate\" : false\n    },\n    \"n\" : {\n      \"count\" : 3\n    }\n  },\n  \"version\" : \"1.0\"\n}";
    let doc = polygo::formats::xcstrings::parse(src).unwrap();
    assert_eq!(polygo::formats::xcstrings::serialize(&doc), src);
    let with_nl = format!("{src}\n");
    let doc = polygo::formats::xcstrings::parse(&with_nl).unwrap();
    assert_eq!(polygo::formats::xcstrings::serialize(&doc), with_nl);
    let crlf = with_nl.replace('\n', "\r\n");
    let doc = polygo::formats::xcstrings::parse(&crlf).unwrap();
    assert_eq!(polygo::formats::xcstrings::serialize(&doc), crlf);
}

fn first_diff(a: &str, b: &str) -> (usize, String, String) {
    for (i, (la, lb)) in a.lines().zip(b.lines()).enumerate() {
        if la != lb {
            return (i + 1, la.to_string(), lb.to_string());
        }
    }
    (
        a.lines().count().min(b.lines().count()) + 1,
        format!("<{} lines>", a.lines().count()),
        format!("<{} lines>", b.lines().count()),
    )
}
