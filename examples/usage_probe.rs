//! Ad-hoc probe: how many of a catalog's keys resolve to code usages, and how fast.
use polygo::context::usage::Index;
fn main() {
    let root = std::env::args().nth(1).expect("root");
    let keys_file = std::env::args().nth(2).expect("keys file");
    let text = std::fs::read_to_string(keys_file).unwrap();
    let keys: Vec<&str> = text.lines().collect();
    let t0 = std::time::Instant::now();
    let index = Index::build(std::path::Path::new(&root)).unwrap();
    let found = index.find_all(&keys);
    let dt = t0.elapsed();
    let with_ident = found.values().filter(|u| u.ident.is_some()).count();
    println!(
        "files={} keys={} resolved={} with_ident={} in {:?}",
        index.files(),
        keys.len(),
        found.len(),
        with_ident,
        dt
    );
    for k in keys.iter().take(400) {
        if let Some(u) = found.get(*k)
            && u.ident.is_some()
            && k.len() < 30
        {
            println!("  {:<28} -> {}:{} in {:?}", k, u.path, u.line, u.ident);
            break;
        }
    }
}
