//! Ad-hoc probe: which few-shot examples would be chosen for a source string in a catalog.
fn main() {
    let path = std::env::args().nth(1).expect("xcstrings");
    let locale = std::env::args().nth(2).expect("locale");
    let source = std::env::args().nth(3).expect("source");
    let doc = polygo::formats::xcstrings::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let units = polygo::formats::xcstrings::units(&doc, "en");
    let candidates: Vec<(String, String)> = units
        .iter()
        .filter_map(|u| {
            u.translations
                .get(&locale)
                .map(|t| (u.source.clone(), t.clone()))
        })
        .collect();
    for (s, t) in polygo::context::fewshot::select(&source, &candidates, 3) {
        println!("{s:?} -> {t:?}");
    }
}
