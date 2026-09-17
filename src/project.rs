//! Load every configured file into units, and write translations back.

use crate::config::{Config, FileSpec, Format};
use crate::core::Unit;
use crate::formats;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Units from all files, keyed as `<file index>:<key>` internally is unnecessary:
/// keys are namespaced by the file path so two files can share a key name.
pub fn load_units(root: &Path, cfg: &Config) -> Result<Vec<Unit>> {
    let mut all = Vec::new();
    for (i, spec) in cfg.files.iter().enumerate() {
        let prefix = if cfg.files.len() > 1 {
            format!("{}:", spec.path.display())
        } else {
            String::new()
        };
        let mut units = load_file_units(root, cfg, spec)
            .with_context(|| format!("files[{i}] {}", spec.path.display()))?;
        // `polygo:skip` in the developer comment removes the key entirely.
        units.retain(|u| !crate::core::directives(u.comment.as_deref()).skip);
        for u in &mut units {
            u.key = format!("{prefix}{}", u.key);
        }
        all.extend(units);
    }
    Ok(all)
}

fn load_file_units(root: &Path, cfg: &Config, spec: &FileSpec) -> Result<Vec<Unit>> {
    let path = root.join(&spec.path);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    match spec.format {
        Format::Xcstrings => {
            let doc = formats::xcstrings::parse(&text)?;
            let mut units = formats::xcstrings::units(&doc, &cfg.source_locale);
            units.extend(formats::xcstrings::plural_units(
                &doc,
                &cfg.source_locale,
                &cfg.target_locales,
            ));
            Ok(units)
        }
        Format::Android => {
            let doc = formats::android::parse(&text)?;
            let mut units: Vec<Unit> = doc
                .entries
                .iter()
                .filter(|e| e.translatable && e.kind == formats::android::Kind::String)
                .map(|e| Unit {
                    key: e.name.clone(),
                    source: e.values[0].text(),
                    comment: e.comment.clone(),
                    translations: BTreeMap::new(),
                    locales: None,
                })
                .collect();
            attach_locale_files(root, cfg, spec, &mut units, |text| {
                let doc = formats::android::parse(text)?;
                Ok(doc
                    .entries
                    .iter()
                    .filter(|e| e.kind == formats::android::Kind::String)
                    .map(|e| (e.name.clone(), e.values[0].text()))
                    .collect())
            })?;
            units.extend(android_plural_units(root, cfg, spec, &doc)?);
            Ok(units)
        }
        Format::Arb => {
            let doc = formats::arb::parse(&text)?;
            let mut units = formats::arb::units(&doc);
            attach_locale_files(root, cfg, spec, &mut units, |text| {
                let doc = formats::arb::parse(text)?;
                Ok(formats::arb::values(&doc))
            })?;
            Ok(units)
        }
        Format::Po => {
            let doc = formats::po::parse(&text)?;
            let mut units = formats::po::units(&doc);
            attach_locale_files(root, cfg, spec, &mut units, |text| {
                Ok(formats::po::values(&formats::po::parse(text)?))
            })?;
            units.extend(po_plural_units(root, cfg, spec, &doc)?);
            Ok(units)
        }
        Format::Resx => {
            let doc = formats::resx::parse(&text)?;
            let mut units = formats::resx::units(&doc);
            attach_locale_files(root, cfg, spec, &mut units, |text| {
                Ok(formats::resx::values(&formats::resx::parse(text)?))
            })?;
            Ok(units)
        }
        Format::Json => {
            let doc = formats::json::parse(&text)?;
            let mut units: Vec<Unit> = doc
                .entries
                .iter()
                .map(|e| Unit {
                    key: e.key(),
                    source: e.text(),
                    comment: None,
                    translations: BTreeMap::new(),
                    locales: None,
                })
                .collect();
            attach_locale_files(root, cfg, spec, &mut units, |text| {
                let doc = formats::json::parse(text)?;
                Ok(doc.entries.iter().map(|e| (e.key(), e.text())).collect())
            })?;
            Ok(units)
        }
    }
}

/// For per-locale formats, read each target locale's file (if present) and attach its
/// values to the matching units.
fn attach_locale_files(
    root: &Path,
    cfg: &Config,
    spec: &FileSpec,
    units: &mut [Unit],
    read: impl Fn(&str) -> Result<BTreeMap<String, String>>,
) -> Result<()> {
    let Some(template) = &spec.locale_path else {
        return Ok(());
    };
    for locale in &cfg.target_locales {
        let path = root.join(locale_file(template, locale));
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let values = read(&text).with_context(|| format!("parsing {}", path.display()))?;
        for u in units.iter_mut() {
            if let Some(v) = values.get(&u.key)
                && !v.is_empty()
            {
                u.translations.insert(locale.clone(), v.clone());
            }
        }
    }
    Ok(())
}

/// Expand a locale path template. `{locale}` is the BCP-47 tag as configured;
/// `{android_locale}` is the Android resource-qualifier form (`pt-BR` → `pt-rBR`,
/// `sr-Latn` → `b+sr+Latn`).
/// Inverse of the key namespacing in `load_units`: `(file index, key within that file)`.
pub fn split_key<'a>(cfg: &Config, key: &'a str) -> (usize, &'a str) {
    if cfg.files.len() > 1 {
        for (i, spec) in cfg.files.iter().enumerate() {
            let prefix = format!("{}:", spec.path.display());
            if let Some(rest) = key.strip_prefix(&prefix) {
                return (i, rest);
            }
        }
    }
    (0, key)
}

pub fn locale_file(template: &str, locale: &str) -> String {
    template
        .replace("{locale}", locale)
        .replace("{android_locale}", &android_qualifier(locale))
}

pub fn android_qualifier(locale: &str) -> String {
    let parts: Vec<&str> = locale.split(['-', '_']).collect();
    match parts.as_slice() {
        [lang] => (*lang).to_string(),
        [lang, region] if region.len() == 2 => format!("{lang}-r{}", region.to_ascii_uppercase()),
        _ => format!("b+{}", parts.join("+")),
    }
}

/// Parse each existing target-locale file once (for plural extraction).
fn locale_docs<T>(
    root: &Path,
    cfg: &Config,
    spec: &FileSpec,
    parse: impl Fn(&str) -> Result<T>,
) -> Result<BTreeMap<String, T>> {
    let mut out = BTreeMap::new();
    let Some(template) = &spec.locale_path else {
        return Ok(out);
    };
    for locale in &cfg.target_locales {
        let path = root.join(locale_file(template, locale));
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        out.insert(
            locale.clone(),
            parse(&text).with_context(|| format!("parsing {}", path.display()))?,
        );
    }
    Ok(out)
}

fn android_plural_units(
    root: &Path,
    cfg: &Config,
    spec: &FileSpec,
    source: &formats::android::Document,
) -> Result<Vec<Unit>> {
    let docs = locale_docs(root, cfg, spec, formats::android::parse)?;
    let forms_of = |doc: &formats::android::Document, name: &str| -> BTreeMap<String, String> {
        doc.entries
            .iter()
            .find(|e| e.kind == formats::android::Kind::Plurals && e.name == name)
            .map(|e| {
                e.values
                    .iter()
                    .filter_map(|v| v.quantity.clone().map(|q| (q, v.text())))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for e in source
        .entries
        .iter()
        .filter(|e| e.translatable && e.kind == formats::android::Kind::Plurals)
    {
        let forms = forms_of(source, &e.name);
        let targets: BTreeMap<String, (Vec<String>, BTreeMap<String, String>)> = cfg
            .target_locales
            .iter()
            .map(|l| {
                let need = crate::check::plurals::required(l)
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect();
                let have = docs
                    .get(l)
                    .map(|d| forms_of(d, &e.name))
                    .unwrap_or_default();
                (l.clone(), (need, have))
            })
            .collect();
        out.extend(crate::core::plural_units(
            &e.name,
            e.comment.as_deref(),
            &forms,
            &targets,
        ));
    }
    Ok(out)
}

fn po_plural_units(
    root: &Path,
    cfg: &Config,
    spec: &FileSpec,
    source: &formats::po::Document,
) -> Result<Vec<Unit>> {
    let docs = locale_docs(root, cfg, spec, formats::po::parse)?;
    let values: BTreeMap<&String, BTreeMap<String, BTreeMap<String, String>>> = docs
        .iter()
        .map(|(l, d)| (l, formats::po::plural_values(d, l)))
        .collect();
    let mut out = Vec::new();
    for (key, forms, comment) in formats::po::plural_sources(source) {
        let targets: BTreeMap<String, (Vec<String>, BTreeMap<String, String>)> = cfg
            .target_locales
            .iter()
            .filter_map(|l| {
                // Slots come from the locale file's own header when it exists, else from
                // the rule polygo would write into a new file.
                let n = docs
                    .get(l)
                    .and_then(formats::po::Document::nplurals)
                    .or_else(|| {
                        formats::po::plural_labels(l, nplurals_of(formats::po::plural_forms(l)))
                            .map(|v| v.len())
                    })?;
                let need: Vec<String> = formats::po::plural_labels(l, n)?
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                let have = values
                    .get(l)
                    .and_then(|m| m.get(&key))
                    .cloned()
                    .unwrap_or_default();
                Some((l.clone(), (need, have)))
            })
            .collect();
        out.extend(crate::core::plural_units(
            &key,
            comment.as_deref(),
            &forms,
            &targets,
        ));
    }
    Ok(out)
}

fn nplurals_of(rule: &str) -> usize {
    rule.split("nplurals=")
        .nth(1)
        .and_then(|s| {
            s.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or(2)
}
