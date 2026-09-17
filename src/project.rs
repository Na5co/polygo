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
            Ok(formats::xcstrings::units(&doc, &cfg.source_locale))
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
