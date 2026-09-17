//! `polygo init` — look at the tree, guess the project type, write `polygo.toml`.
//!
//! Detection is deliberately boring: it looks for the files each ecosystem
//! actually ships (`.xcstrings`, `res/values*/strings.xml`, `*.arb`,
//! `locales/<lang>/*.json` or `locales/<lang>.json`) and derives source and
//! target locales from what already exists.

use crate::config::{Config, FileSpec, Format, Provider};
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
            "no localization files found under {} (looked for .xcstrings, res/values/strings.xml, .arb, locales/*.json)",
            root.display()
        );
    }
    specs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Config {
        source_locale: source.unwrap_or_else(|| "en".into()),
        target_locales: targets.into_iter().collect(),
        files: specs,
        provider: Provider::default(),
        glossary: None,
        batch_size: 20,
        jobs: 1,
        length_ratio: 2.5,
        context: true,
        context_tokens: 600,
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
            out.push(rel.to_path_buf());
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
    parts[1..].iter().all(|p| {
        (p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()))
            || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()))
            || (p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic()))
    })
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
