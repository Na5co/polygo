//! `polygo init`: look at the tree, guess the project type, write `polygo.toml`.
//!
//! Detection is deliberately boring: it looks for the files each ecosystem
//! actually ships (`.xcstrings`, `res/values*/strings.xml`, `*.arb`,
//! `locales/<lang>/*.json` or `locales/<lang>.json`) and derives source and
//! target locales from what already exists.

use crate::config::{Config, FileSpec, Format};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "Pods",
    "build",
    ".build",
    "DerivedData",
    "target",
    "dist",
    ".next",
    "vendor",
    ".dart_tool",
    "Carthage",
    ".gradle",
    "out",
];

pub fn detect(root: &Path) -> Result<Config> {
    let files = walk(root);
    let mut specs: Vec<FileSpec> = Vec::new();
    let mut source: Option<String> = None;
    let mut targets: BTreeSet<String> = BTreeSet::new();

    // iOS / macOS String Catalogs.
    for rel in files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "xcstrings"))
    {
        if let Ok(text) = std::fs::read_to_string(root.join(rel))
            && let Ok(doc) = crate::formats::xcstrings::parse(&text)
        {
            let src = doc
                .root
                .get("sourceLanguage")
                .and_then(|v| v.as_str())
                .unwrap_or("en")
                .to_string();
            if let Some(strings) = doc.root.get("strings").and_then(|v| v.as_object()) {
                for entry in strings.values() {
                    if let Some(locs) = entry.get("localizations").and_then(|v| v.as_object()) {
                        for l in locs.keys() {
                            if *l != src {
                                targets.insert(l.clone());
                            }
                        }
                    }
                }
            }
            source.get_or_insert(src);
            specs.push(FileSpec {
                format: Format::Xcstrings,
                path: rel.clone(),
                locale_path: None,
            });
        }
    }

    // Android resource directories.
    for rel in files.iter().filter(|p| {
        p.file_name().is_some_and(|f| f == "strings.xml")
            && p.parent()
                .and_then(|d| d.file_name())
                .is_some_and(|d| d == "values")
    }) {
        let values_dir = rel.parent().unwrap();
        let res_dir = values_dir.parent().unwrap_or(Path::new(""));
        if let Ok(entries) = std::fs::read_dir(root.join(res_dir)) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(loc) = android_dir_locale(&name)
                    && e.path().join("strings.xml").exists()
                {
                    targets.insert(loc);
                }
            }
        }
        source.get_or_insert_with(|| "en".to_string());
        specs.push(FileSpec {
            format: Format::Android,
            path: rel.clone(),
            locale_path: Some(format!(
                "{}/values-{{android_locale}}/strings.xml",
                res_dir.display()
            )),
        });
    }

    // Flutter ARB.
    let arbs: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "arb"))
        .collect();
    if !arbs.is_empty() {
        let l10n = std::fs::read_to_string(root.join("l10n.yaml")).unwrap_or_default();
        let template = yaml_value(&l10n, "template-arb-file");
        let mut by_dir: BTreeMap<PathBuf, Vec<(String, String)>> = BTreeMap::new(); // dir → (stem, locale)
        for p in &arbs {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let Some((prefix, loc)) = split_locale_suffix(&stem) else {
                continue;
            };
            by_dir
                .entry(p.parent().unwrap().to_path_buf())
                .or_default()
                .push((prefix, loc));
        }
        for (dir, items) in by_dir {
            let src_loc = template
                .as_deref()
                .and_then(|t| split_locale_suffix(t.trim_end_matches(".arb")).map(|(_, l)| l))
                .or_else(|| {
                    items
                        .iter()
                        .find(|(_, l)| l == "en")
                        .map(|(_, l)| l.clone())
                })
                .unwrap_or_else(|| items[0].1.clone());
            let prefix = items
                .iter()
                .find(|(_, l)| *l == src_loc)
                .map(|(p, _)| p.clone())
                .unwrap_or_default();
            for (_, l) in &items {
                if *l != src_loc {
                    targets.insert(l.clone());
                }
            }
            source.get_or_insert(src_loc.clone());
            specs.push(FileSpec {
                format: Format::Arb,
                path: dir.join(format!("{prefix}{src_loc}.arb")),
                locale_path: Some(format!("{}/{prefix}{{locale}}.arb", dir.display())),
            });
        }
    }

    // gettext: <dir>/<locale>/LC_MESSAGES/<domain>.po and <dir>/<locale>.po.
    let pos: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "po"))
        .collect();
    let mut po_groups: BTreeMap<(PathBuf, String), BTreeSet<String>> = BTreeMap::new(); // (base dir, domain) → locales
    let mut po_flat: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for p in &pos {
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let comps: Vec<String> = p
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if comps.len() >= 4
            && comps[comps.len() - 2] == "LC_MESSAGES"
            && is_locale(&comps[comps.len() - 3])
        {
            let base: PathBuf = comps[..comps.len() - 3].iter().collect();
            po_groups
                .entry((base, stem))
                .or_default()
                .insert(comps[comps.len() - 3].clone());
        } else if is_locale(&stem) {
            po_flat
                .entry(p.parent().unwrap().to_path_buf())
                .or_default()
                .insert(stem);
        }
    }
    for ((base, domain), locales) in po_groups {
        let src_loc = pick_source(locales.iter(), source.as_deref());
        specs.push(FileSpec {
            format: Format::Po,
            path: base
                .join(&src_loc)
                .join("LC_MESSAGES")
                .join(format!("{domain}.po")),
            locale_path: Some(format!(
                "{}/{{locale}}/LC_MESSAGES/{domain}.po",
                base.display()
            )),
        });
        for l in &locales {
            if *l != src_loc {
                targets.insert(l.clone());
            }
        }
        source.get_or_insert(src_loc);
    }
    for (dir, locales) in po_flat {
        let src_loc = pick_source(locales.iter(), source.as_deref());
        specs.push(FileSpec {
            format: Format::Po,
            path: dir.join(format!("{src_loc}.po")),
            locale_path: Some(format!("{}/{{locale}}.po", dir.display())),
        });
        for l in &locales {
            if *l != src_loc {
                targets.insert(l.clone());
            }
        }
        source.get_or_insert(src_loc);
    }

    // .NET: Name.resx + Name.<locale>.resx, and <dir>/<locale>/Resources.resw.
    let resxs: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "resx" || e == "resw"))
        .collect();
    let mut resx_groups: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new(); // base file (no locale) → locales
    let mut resw_groups: BTreeMap<(PathBuf, String), BTreeSet<String>> = BTreeMap::new(); // (parent, file name) → locale dirs
    for p in &resxs {
        let ext = p.extension().unwrap().to_string_lossy().into_owned();
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();
        let dir_name = dir
            .file_name()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_locale(&dir_name) {
            resw_groups
                .entry((dir.parent().unwrap().to_path_buf(), format!("{stem}.{ext}")))
                .or_default()
                .insert(dir_name);
        } else if let Some((base, loc)) = stem.rsplit_once('.').filter(|(_, l)| is_locale(l)) {
            resx_groups
                .entry(dir.join(format!("{base}.{ext}")))
                .or_default()
                .insert(loc.to_string());
        } else {
            resx_groups.entry(p.to_path_buf()).or_default();
        }
    }
    for (base_file, locales) in resx_groups {
        if !root.join(&base_file).exists() {
            continue;
        }
        let stem = base_file
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let ext = base_file
            .extension()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        specs.push(FileSpec {
            format: Format::Resx,
            path: base_file.clone(),
            locale_path: Some(format!(
                "{}/{stem}.{{locale}}.{ext}",
                base_file.parent().unwrap().display()
            )),
        });
        targets.extend(locales);
        source.get_or_insert_with(|| "en".to_string());
    }
    for ((parent, file), locales) in resw_groups {
        let src_loc = pick_source(locales.iter(), source.as_deref());
        specs.push(FileSpec {
            format: Format::Resx,
            path: parent.join(&src_loc).join(&file),
            locale_path: Some(format!("{}/{{locale}}/{file}", parent.display())),
        });
        for l in &locales {
            if *l != src_loc {
                targets.insert(l.clone());
            }
        }
        source.get_or_insert(src_loc);
    }

    // JSON: <dir>/<locale>/<ns>.json and <dir>/<locale>.json.
    let jsons: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name().is_some_and(|f| f != "package.json")
        })
        .collect();
    let mut ns_groups: BTreeMap<PathBuf, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new(); // parent → locale → namespaces
    let mut flat_groups: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new(); // dir → locales
    for p in &jsons {
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();
        // Only files whose leaves are all strings look like translation files; a
        // `tsconfig.json` under `packages/<name>/` does not, whatever the dir is called.
        if !std::fs::read_to_string(root.join(p)).is_ok_and(|t| looks_like_locale_json(&t)) {
            continue;
        }
        let dir_name = dir
            .file_name()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_locale(&dir_name) {
            ns_groups
                .entry(dir.parent().unwrap().to_path_buf())
                .or_default()
                .entry(dir_name)
                .or_default()
                .insert(stem);
        } else if is_locale(&stem) {
            flat_groups
                .entry(dir.to_path_buf())
                .or_default()
                .insert(stem);
        }
    }
    for (parent, locales) in ns_groups {
        let src_loc = pick_source(locales.keys(), source.as_deref());
        for ns in &locales[&src_loc] {
            specs.push(FileSpec {
                format: Format::Json,
                path: parent.join(&src_loc).join(format!("{ns}.json")),
                locale_path: Some(format!("{}/{{locale}}/{ns}.json", parent.display())),
            });
        }
        for l in locales.keys() {
            if *l != src_loc {
                targets.insert(l.clone());
            }
        }
        source.get_or_insert(src_loc);
    }
    for (dir, locales) in flat_groups {
        // A single `en.json` counts only inside a directory that is clearly for locales,
        // so a stray `config/en.json` is not mistaken for a translation file.
        let dir_name = dir
            .file_name()
            .map(|d| d.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let locale_dir = [
            "locales",
            "locale",
            "i18n",
            "lang",
            "langs",
            "languages",
            "translations",
            "messages",
            "l10n",
        ]
        .contains(&dir_name.as_str());
        if locales.len() < 2 && !locale_dir {
            continue;
        }
        let src_loc = pick_source(locales.iter(), source.as_deref());
        specs.push(FileSpec {
            format: Format::Json,
            path: dir.join(format!("{src_loc}.json")),
            locale_path: Some(format!("{}/{{locale}}.json", dir.display())),
        });
        for l in &locales {
            if *l != src_loc {
                targets.insert(l.clone());
            }
        }
        source.get_or_insert(src_loc);
    }

    if specs.is_empty() {
        bail!(
            "no localization files found under {} (looked for .xcstrings, res/values/strings.xml, .arb, locales/*.json, .po, .resx/.resw)",
            root.display()
        );
    }
    // polygo.toml is shared across machines: always forward slashes, whatever
    // `Path::join` produced on this one.
    for spec in &mut specs {
        spec.path = PathBuf::from(spec.path.to_string_lossy().replace('\\', "/"));
        if let Some(lp) = &mut spec.locale_path {
            *lp = lp.replace('\\', "/");
        }
    }
    specs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Config {
        source_locale: source.unwrap_or_else(|| "en".into()),
        target_locales: targets.into_iter().collect(),
        files: specs,
        provider: crate::models::default_provider().unwrap_or_default(),
        glossary: None,
        batch_size: 20,
        jobs: 1,
        length_ratio: 2.5,
        context: true,
        context_tokens: 600,
        memory: true,
    })
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_some_and(|t| t.is_dir()) && SKIP_DIRS.contains(&name.as_ref()))
        })
        .build();
    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|t| t.is_file())
            && let Ok(rel) = entry.path().strip_prefix(root)
        {
            // polygo.toml is shared across machines: always forward slashes.
            out.push(PathBuf::from(rel.to_string_lossy().replace('\\', "/")));
        }
    }
    out.sort();
    out
}

fn pick_source<'a>(locales: impl Iterator<Item = &'a String>, preferred: Option<&str>) -> String {
    let all: Vec<&String> = locales.collect();
    if let Some(p) = preferred
        && let Some(l) = all.iter().find(|l| l.as_str() == p)
    {
        return (*l).clone();
    }
    all.iter()
        .find(|l| l.as_str() == "en" || l.starts_with("en-") || l.starts_with("en_"))
        .or(all.first())
        .map(|l| (*l).clone())
        .unwrap_or_else(|| "en".into())
}

/// `values-de` → `de`, `values-pt-rBR` → `pt-BR`, `values-b+sr+Latn` → `sr-Latn`;
/// qualifier-only dirs (`values-night`, `values-sw600dp`, `values-v21`) → None.
pub fn android_dir_locale(dir: &str) -> Option<String> {
    let rest = dir.strip_prefix("values-")?;
    if let Some(bcp) = rest.strip_prefix("b+") {
        return Some(bcp.replace('+', "-"));
    }
    let mut parts = rest.split('-');
    let lang = parts.next()?;
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    match parts.next() {
        None => Some(lang.to_string()),
        Some(region)
            if region.len() == 3
                && region.starts_with('r')
                && region[1..].bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            if parts.next().is_some() {
                return None; // further qualifiers → not a plain locale dir
            }
            Some(format!("{lang}-{}", &region[1..]))
        }
        Some(_) => None,
    }
}

/// `app_en` → (`app_`, `en`); `intl_pt_BR` → (`intl_`, `pt_BR`); `en` → (``, `en`).
fn split_locale_suffix(stem: &str) -> Option<(String, String)> {
    // Prefer a separator split (`app_en` → `app_` + `en`) over reading the whole stem
    // as a locale, because `app_en` itself also looks like `<lang>_<region>`.
    for (i, c) in stem.char_indices() {
        if (c == '_' || c == '-') && is_locale(&stem[i + 1..]) {
            return Some((stem[..=i].to_string(), stem[i + 1..].to_string()));
        }
    }
    if is_locale(stem) {
        return Some((String::new(), stem.to_string()));
    }
    None
}

/// Loose BCP-47-ish check: `en`, `pt-BR`, `pt_BR`, `zh-Hans`, `sr-Latn-RS`, `en-us`.
pub fn is_locale(s: &str) -> bool {
    let parts: Vec<&str> = s.split(['-', '_']).collect();
    if parts.is_empty() || parts.len() > 3 {
        return false;
    }
    let lang = parts[0];
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }
    // A real language subtag, not any three lowercase letters (`cli`, `mcp`, `src`).
    if !LANGUAGES.contains(&lang) {
        return false;
    }
    parts[1..].iter().all(|p| {
        (p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()))
            || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()))
            || (p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic()))
    })
}

/// ISO 639-1 codes plus the three-letter ones that ship in apps (`fil`, `haw`, `ast`,
/// `ceb`, `kab`, `tzm`, `yue`, `cnr`, `sat`, `nds`, `frp`, `szl`…).
const LANGUAGES: &[&str] = &[
    "aa", "ab", "ae", "af", "ak", "am", "an", "ar", "as", "av", "ay", "az", "ba", "be", "bg", "bh",
    "bi", "bm", "bn", "bo", "br", "bs", "ca", "ce", "ch", "co", "cr", "cs", "cu", "cv", "cy", "da",
    "de", "dv", "dz", "ee", "el", "en", "eo", "es", "et", "eu", "fa", "ff", "fi", "fj", "fo", "fr",
    "fy", "ga", "gd", "gl", "gn", "gu", "gv", "ha", "he", "hi", "ho", "hr", "ht", "hu", "hy", "hz",
    "ia", "id", "ie", "ig", "ii", "ik", "in", "io", "is", "it", "iu", "iw", "ja", "ji", "jv", "jw",
    "ka", "kg", "ki", "kj", "kk", "kl", "km", "kn", "ko", "kr", "ks", "ku", "kv", "kw", "ky", "la",
    "lb", "lg", "li", "ln", "lo", "lt", "lu", "lv", "mg", "mh", "mi", "mk", "ml", "mn", "mo", "mr",
    "ms", "mt", "my", "na", "nb", "nd", "ne", "ng", "nl", "nn", "no", "nr", "nv", "ny", "oc", "oj",
    "om", "or", "os", "pa", "pi", "pl", "ps", "pt", "qu", "rm", "rn", "ro", "ru", "rw", "sa", "sc",
    "sd", "se", "sg", "sh", "si", "sk", "sl", "sm", "sn", "so", "sq", "sr", "ss", "st", "su", "sv",
    "sw", "ta", "te", "tg", "th", "ti", "tk", "tl", "tn", "to", "tr", "ts", "tt", "tw", "ty", "ug",
    "uk", "ur", "uz", "ve", "vi", "vo", "wa", "wo", "xh", "yi", "yo", "za", "zh", "zu", "ast",
    "ceb", "fil", "haw", "kab", "tzm", "yue", "cnr", "sat", "nds", "frp", "szl", "ckb", "hsb",
    "dsb", "kea", "mai", "mni", "sma", "smj", "smn", "sms", "wae", "ars", "prg", "brx", "doi",
    "kok", "syr", "tok",
];

/// A JSON object whose leaves are strings (nested objects and string arrays allowed).
fn looks_like_locale_json(text: &str) -> bool {
    fn leaves(v: &serde_json::Value, strings: &mut usize) -> bool {
        match v {
            serde_json::Value::String(_) => {
                *strings += 1;
                true
            }
            serde_json::Value::Object(m) => m.values().all(|x| leaves(x, strings)),
            serde_json::Value::Array(a) => a.iter().all(|x| leaves(x, strings)),
            _ => false,
        }
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    let mut n = 0;
    v.is_object() && leaves(&v, &mut n) && n > 0
}

fn yaml_value(yaml: &str, key: &str) -> Option<String> {
    yaml.lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix(key)
                .and_then(|r| r.trim().strip_prefix(':'))
        })
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|v| !v.is_empty())
}
