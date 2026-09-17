//! `polygo.lock` — what has been translated, from which source text, by what.
//!
//! The lockfile lets `translate` touch only keys whose source changed and lets
//! `status` explain the state of every key in every locale. It is TOML, sorted
//! by key and locale, so diffs in git are stable and reviewable.

use crate::core::{Unit, hash};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FILE_NAME: &str = "polygo.lock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleRecord {
    /// Hash of the translation as polygo last wrote it.
    pub hash: String,
    pub provider: String,
    pub model: String,
    /// RFC 3339 UTC timestamp of when it was written.
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KeyRecord {
    /// Hash of the source text the translations were made from.
    pub source: String,
    #[serde(default)]
    pub locales: BTreeMap<String, LocaleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Lock {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub keys: BTreeMap<String, KeyRecord>,
}

fn default_version() -> u32 {
    1
}

/// State of one key in one locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum State {
    /// Key never recorded in the lockfile.
    New,
    /// Recorded, but the source text changed since.
    Stale,
    /// Recorded, but the locale has no translation any more.
    Untranslated,
    /// Translation exists but was not written by polygo (hand-edited or pre-existing).
    /// Never overwritten.
    Edited,
    /// Translation matches the lockfile and the source is unchanged.
    UpToDate,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::New => "new",
            State::Stale => "stale",
            State::Untranslated => "untranslated",
            State::Edited => "edited",
            State::UpToDate => "up-to-date",
        }
    }

    pub const ALL: [State; 5] = [
        State::New,
        State::Stale,
        State::Untranslated,
        State::Edited,
        State::UpToDate,
    ];
}

#[derive(Debug, Default, Clone)]
pub struct Status {
    /// locale → key → state
    pub per_locale: BTreeMap<String, BTreeMap<String, State>>,
}

impl Status {
    pub fn count(&self, locale: &str, state: State) -> usize {
        self.per_locale
            .get(locale)
            .map(|m| m.values().filter(|s| **s == state).count())
            .unwrap_or(0)
    }

    pub fn keys(&self, locale: &str, state: State) -> Vec<String> {
        self.per_locale
            .get(locale)
            .map(|m| {
                m.iter()
                    .filter(|(_, s)| **s == state)
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Keys that `translate` should (re)translate for a locale: new, stale, untranslated.
    pub fn work(&self, locale: &str) -> Vec<String> {
        self.per_locale
            .get(locale)
            .map(|m| {
                m.iter()
                    .filter(|(_, s)| matches!(s, State::New | State::Stale | State::Untranslated))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Lock {
    pub fn from_toml(text: &str) -> Result<Lock> {
        toml::from_str(text).context("invalid polygo.lock")
    }

    pub fn to_toml(&self) -> String {
        let mut out = format!(
            "# polygo lockfile — generated, but commit it.\nversion = {}\n",
            self.version
        );
        for (key, rec) in &self.keys {
            out.push_str(&format!(
                "\n[keys.{}]\nsource = \"{}\"\n",
                toml_key(key),
                rec.source
            ));
            for (locale, l) in &rec.locales {
                out.push_str(&format!(
                    "\n[keys.{}.locales.{}]\nhash = \"{}\"\nprovider = \"{}\"\nmodel = \"{}\"\nat = \"{}\"\n",
                    toml_key(key),
                    toml_key(locale),
                    l.hash,
                    escape(&l.provider),
                    escape(&l.model),
                    l.at
                ));
            }
        }
        out
    }

    pub fn load(path: &std::path::Path) -> Result<Lock> {
        if !path.exists() {
            return Ok(Lock::default());
        }
        Lock::from_toml(&std::fs::read_to_string(path).context("reading polygo.lock")?)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        std::fs::write(path, self.to_toml()).context("writing polygo.lock")
    }

    /// Compute the state of every unit in every target locale.
    pub fn status(&self, units: &[Unit], locales: &[&str]) -> Status {
        let mut status = Status::default();
        for locale in locales {
            let map = status.per_locale.entry((*locale).to_string()).or_default();
            for u in units {
                let state = match self.keys.get(&u.key) {
                    // Unknown to the lockfile: an existing translation came from a human or
                    // another tool and is never overwritten; no translation means work to do.
                    None => {
                        if u.translations.contains_key(*locale) {
                            State::Edited
                        } else {
                            State::New
                        }
                    }
                    Some(rec) => {
                        let translation = u.translations.get(*locale);
                        match (rec.locales.get(*locale), translation) {
                            // Never translated into this locale: treat like a new key.
                            (None, None) => State::New,
                            // Was translated before, but the translation is gone now.
                            (Some(_), None) => State::Untranslated,
                            (None, Some(_)) => State::Edited,
                            (Some(l), Some(t)) => {
                                if l.hash != hash(t) {
                                    State::Edited
                                } else if rec.source != hash(&u.source) {
                                    State::Stale
                                } else {
                                    State::UpToDate
                                }
                            }
                        }
                    }
                };
                map.insert(u.key.clone(), state);
            }
        }
        status
    }

    /// Record the current source hash of every unit and the translation hash for every
    /// locale that has one. Keys no longer present are pruned.
    pub fn record_all(&mut self, units: &[Unit], locales: &[&str], provider: &str, model: &str) {
        let at = now_rfc3339();
        let mut keys = BTreeMap::new();
        for u in units {
            let mut rec = self.keys.remove(&u.key).unwrap_or_default();
            rec.source = hash(&u.source);
            for locale in locales {
                if let Some(t) = u.translations.get(*locale) {
                    rec.locales.insert(
                        (*locale).to_string(),
                        LocaleRecord {
                            hash: hash(t),
                            provider: provider.to_string(),
                            model: model.to_string(),
                            at: at.clone(),
                        },
                    );
                }
            }
            keys.insert(u.key.clone(), rec);
        }
        self.keys = keys;
    }

    /// Record a single translation polygo just wrote.
    pub fn record(
        &mut self,
        unit: &Unit,
        locale: &str,
        translation: &str,
        provider: &str,
        model: &str,
    ) {
        let rec = self.keys.entry(unit.key.clone()).or_default();
        rec.source = hash(&unit.source);
        rec.locales.insert(
            locale.to_string(),
            LocaleRecord {
                hash: hash(translation),
                provider: provider.to_string(),
                model: model.to_string(),
                at: now_rfc3339(),
            },
        );
    }
}

fn toml_key(k: &str) -> String {
    if !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        k.to_string()
    } else {
        format!("\"{}\"", escape(k))
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Current UTC time as RFC 3339 with second precision, without pulling in a date crate.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}
