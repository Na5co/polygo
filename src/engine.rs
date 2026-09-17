//! The translate loop: work out what needs doing, batch it, call the provider with
//! retries, write results back to the files and the lockfile after every batch so
//! an interrupted run resumes exactly where it stopped.

use crate::config::{Config, FileSpec, Format};
use crate::core::Unit;
use crate::formats;
use crate::lockfile::Lock;
use crate::project;
use crate::provider::{Ctx, Provider, Request};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};

#[derive(Debug, Clone)]
pub struct Options {
    /// Restrict to these locales (default: all configured targets).
    pub locales: Option<Vec<String>>,
    pub batch_size: usize,
    pub jobs: usize,
    pub dry_run: bool,
    pub verbose: bool,
    /// Also retry keys that were quarantined on a previous run.
    pub retry_review: bool,
    /// Translate exactly these keys per locale regardless of lockfile state (used by `check --fix`).
    pub force_keys: Option<BTreeMap<String, Vec<String>>>,
    /// Attach code-usage context and few-shot examples (config `context`, `--no-context` overrides).
    pub context: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub translated: usize,
    pub batches: usize,
    pub per_locale: BTreeMap<String, usize>,
    pub planned: BTreeMap<String, Vec<String>>,
    /// (locale, key, reason) for everything quarantined this run.
    pub review: Vec<(String, String, String)>,
}

pub fn translate(
    root: &Path,
    cfg: &Config,
    provider: &(dyn Provider + Sync),
    opts: &Options,
) -> Result<Report> {
    let units = project::load_units(root, cfg)?;
    let lock_path = root.join(crate::lockfile::FILE_NAME);
    let mut lock = Lock::load(&lock_path)?;
    let locales: Vec<String> = match &opts.locales {
        Some(l) => l.clone(),
        None => cfg.target_locales.clone(),
    };
    for l in &locales {
        if !cfg.target_locales.contains(l) {
            anyhow::bail!(
                "locale `{l}` is not in target_locales ({})",
                cfg.target_locales.join(", ")
            );
        }
    }
    let locale_refs: Vec<&str> = locales.iter().map(String::as_str).collect();
    let status = lock.status(&units, &locale_refs);
    let by_key: BTreeMap<&str, &Unit> = units.iter().map(|u| (u.key.as_str(), u)).collect();

    let mut report = Report::default();
    for locale in &locales {
        let work = match &opts.force_keys {
            Some(forced) => forced
                .get(locale)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|k| by_key.contains_key(k.as_str()))
                .collect(),
            None => status.work_with(locale, opts.retry_review),
        };
        report.planned.insert(locale.clone(), work.clone());
    }
    if opts.dry_run {
        return Ok(report);
    }

    let mut ws = Workspace::open(root, cfg)?;
    let glossary = crate::glossary::load(root, cfg)?;
    // Context retrieval: index the source tree once, resolve every key that has work.
    let usage_index = if opts.context {
        Some(crate::context::usage::Index::build(root)?)
    } else {
        None
    };
    let usages: std::collections::HashMap<String, crate::context::usage::Usage> = match &usage_index
    {
        Some(index) => {
            let all: Vec<&str> = report
                .planned
                .values()
                .flatten()
                .map(|k| crate::core::base_key(project::split_key(cfg, k).1))
                .collect();
            let found = index.find_all(&all);
            report
                .planned
                .values()
                .flatten()
                .filter_map(|k| {
                    found
                        .get(crate::core::base_key(project::split_key(cfg, k).1))
                        .map(|u| (k.clone(), u.clone()))
                })
                .collect()
        }
        None => Default::default(),
    };
    let budget = crate::context::assemble::Budget {
        tokens: cfg.context_tokens,
    };
    let batch_size = opts.batch_size.max(1);
    let jobs = opts.jobs.max(1);

    for locale in &locales {
        let work = &report.planned[locale];
        if work.is_empty() {
            continue;
        }
        let ctx = Ctx {
            source_locale: cfg.source_locale.clone(),
            target_locale: locale.clone(),
            glossary: glossary.terms_for(locale),
            do_not_translate: glossary.do_not_translate.clone(),
            format_hint: Some(ws.format_hint()),
        };
        // Build batches of requests.
        let batches: VecDeque<(usize, Vec<Request>)> = work
            .chunks(batch_size)
            .enumerate()
            .map(|(i, keys)| {
                (
                    i,
                    keys.iter()
                        .map(|k| {
                            let u = by_key[k.as_str()];
                            let mut r = Request {
                                key: u.key.clone(),
                                source: u.source.clone(),
                                comment: u.comment.clone(),
                                context: None,
                                examples: vec![],
                            };
                            if opts.context {
                                let candidates: Vec<(String, String)> = units
                                    .iter()
                                    .filter_map(|o| {
                                        o.translations
                                            .get(locale)
                                            .map(|t| (o.source.clone(), t.clone()))
                                    })
                                    .collect();
                                let examples =
                                    crate::context::fewshot::select(&u.source, &candidates, 3);
                                crate::context::assemble::attach(
                                    &mut r,
                                    usages.get(&u.key),
                                    &examples,
                                    &budget,
                                );
                            }
                            r
                        })
                        .collect(),
                )
            })
            .collect();
        let total = batches.len();
        let queue = Mutex::new(batches);
        type Msg = (
            usize,
            Result<crate::provider::Outcome>,
            mpsc::SyncSender<()>,
        );
        let (tx, rx) = mpsc::channel::<Msg>();

        std::thread::scope(|s| {
            for _ in 0..jobs.min(total) {
                let tx = tx.clone();
                let queue = &queue;
                let ctx = &ctx;
                s.spawn(move || {
                    loop {
                        let next = queue.lock().unwrap().pop_front();
                        let Some((i, batch)) = next else { break };
                        let result = translate_with_retry(provider, &batch, ctx);
                        // Hand the result over and wait until it has been persisted before
                        // starting the next batch, so a crash can only lose in-flight work.
                        let (ack_tx, ack_rx) = mpsc::sync_channel::<()>(0);
                        if tx.send((i, result, ack_tx)).is_err() || ack_rx.recv().is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            // Apply results as they arrive; persist after every batch, then ack.
            for _ in 0..total {
                let (i, result, ack) = rx.recv().context("provider worker died")?;
                let outcome =
                    result.with_context(|| format!("{locale}: batch {}/{total} failed", i + 1))?;
                for t in outcome.translations {
                    let u = by_key[t.key.as_str()];
                    ws.set(cfg, &t.key, locale, &t.text)?;
                    lock.record(u, locale, &t.text, provider.name(), provider.model());
                    if opts.verbose {
                        eprintln!("  {locale}  {}  →  {}", u.key, t.text);
                    }
                    report.translated += 1;
                    *report.per_locale.entry(locale.clone()).or_default() += 1;
                }
                for r in outcome.review {
                    let u = by_key[r.key.as_str()];
                    lock.quarantine(u, locale, &r.reason, r.suggestion.as_deref());
                    report
                        .review
                        .push((locale.clone(), r.key.clone(), r.reason.clone()));
                }
                ws.flush()?;
                lock.save(&lock_path)?;
                report.batches += 1;
                let _ = ack.send(());
            }
            Ok::<(), anyhow::Error>(())
        })?;
    }
    // Make sure every source hash is recorded even for keys that needed no work.
    for u in &units {
        lock.keys.entry(u.key.clone()).or_default().source = crate::core::hash(&u.source);
    }
    lock.save(&lock_path)?;
    Ok(report)
}

/// Write one translation through the format serializers (used by `review`).
pub fn write_translation(
    root: &Path,
    cfg: &Config,
    key: &str,
    locale: &str,
    text: &str,
) -> Result<()> {
    let mut ws = Workspace::open(root, cfg)?;
    ws.set(cfg, key, locale, text)?;
    ws.flush()
}

fn translate_with_retry(
    provider: &dyn Provider,
    batch: &[Request],
    ctx: &Ctx,
) -> Result<crate::provider::Outcome> {
    let mut last = None;
    for attempt in 0..3u32 {
        match provider.translate(batch, ctx) {
            Ok(t) => return Ok(t),
            Err(e) => {
                if attempt < 2 && !std::env::var("POLYGO_NO_BACKOFF").is_ok_and(|v| v == "1") {
                    std::thread::sleep(std::time::Duration::from_millis(500 * 2u64.pow(attempt)));
                }
                last = Some(e);
            }
        }
    }
    Err(last.unwrap().context("gave up after 3 attempts"))
}

// ---- workspace: loaded documents per file spec, written back on flush ----------------

enum Loaded {
    Xcstrings {
        path: PathBuf,
        doc: formats::xcstrings::Document,
        dirty: bool,
    },
    Android {
        template: String,
        locales: BTreeMap<String, (PathBuf, formats::android::Document, bool)>,
    },
    Json {
        source: serde_json::Value,
        style: formats::json::Style,
        template: String,
        locales: BTreeMap<String, JsonLocale>,
    },
    Arb {
        source_text: String,
        template: String,
        /// locale → (path, existing text, new values, dirty)
        locales: BTreeMap<String, ArbLocale>,
    },
    /// .po and .resx: one document per locale, spliced or appended.
    PerLocale {
        format: Format,
        source_text: String,
        source_units: Vec<crate::core::Unit>,
        template: String,
        locales: BTreeMap<String, (PathBuf, PerLocaleDoc, bool)>,
    },
}

enum PerLocaleDoc {
    Po(formats::po::Document),
    Resx(formats::resx::Document),
}

struct ArbLocale {
    path: PathBuf,
    locale: String,
    existing: Option<String>,
    values: BTreeMap<String, String>,
    dirty: bool,
}

struct JsonLocale {
    path: PathBuf,
    existing: Option<serde_json::Value>,
    values: BTreeMap<String, String>,
    dirty: bool,
}

struct Workspace {
    root: PathBuf,
    files: Vec<Loaded>,
    formats: Vec<Format>,
}

impl Workspace {
    fn open(root: &Path, cfg: &Config) -> Result<Workspace> {
        let mut files = Vec::new();
        for spec in &cfg.files {
            files.push(Self::load(root, spec)?);
        }
        Ok(Workspace {
            root: root.to_path_buf(),
            files,
            formats: cfg.files.iter().map(|f| f.format).collect(),
        })
    }

    fn load(root: &Path, spec: &FileSpec) -> Result<Loaded> {
        let path = root.join(&spec.path);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(match spec.format {
            Format::Xcstrings => Loaded::Xcstrings {
                path,
                doc: formats::xcstrings::parse(&text)?,
                dirty: false,
            },
            Format::Android => Loaded::Android {
                template: spec.locale_path.clone().context(
                    "android files need locale_path (e.g. res/values-{android_locale}/strings.xml)",
                )?,
                locales: BTreeMap::new(),
            },
            Format::Json => Loaded::Json {
                source: serde_json::from_str(&text).context("source JSON")?,
                style: formats::json::Style::detect(&text),
                template: spec
                    .locale_path
                    .clone()
                    .context("json files need locale_path (e.g. locales/{locale}.json)")?,
                locales: BTreeMap::new(),
            },
            Format::Po | Format::Resx => Loaded::PerLocale {
                format: spec.format,
                source_units: match spec.format {
                    Format::Po => formats::po::units(&formats::po::parse(&text)?),
                    _ => formats::resx::units(&formats::resx::parse(&text)?),
                },
                source_text: text,
                template: spec
                    .locale_path
                    .clone()
                    .context("po/resx files need locale_path")?,
                locales: BTreeMap::new(),
            },
            Format::Arb => Loaded::Arb {
                source_text: text,
                template: spec
                    .locale_path
                    .clone()
                    .context("arb files need locale_path (e.g. lib/l10n/app_{locale}.arb)")?,
                locales: BTreeMap::new(),
            },
        })
    }

    fn format_hint(&self) -> String {
        let mut hints: Vec<&str> = Vec::new();
        for f in &self.formats {
            let h = match f {
                Format::Xcstrings => "Apple format specifiers like %@, %d, %lld, %1$@",
                Format::Android => "Android specifiers like %s, %d, %1$s and inline tags",
                Format::Json => "i18next interpolations like {{name}} and $t(key)",
                Format::Arb => "ICU placeholders like {name}",
                Format::Po => "printf placeholders like %s, %(name)s, {0}",
                Format::Resx => ".NET placeholders like {0}, {name}",
            };
            if !hints.contains(&h) {
                hints.push(h);
            }
        }
        hints.join("; ")
    }

    fn set(&mut self, cfg: &Config, key: &str, locale: &str, text: &str) -> Result<()> {
        let (idx, local_key) = project::split_key(cfg, key);
        let plural = crate::core::split_plural(local_key);
        let root = self.root.clone();
        match &mut self.files[idx] {
            Loaded::Xcstrings { doc, dirty, .. } => {
                let ok = match plural {
                    Some((base, cat)) => formats::xcstrings::set_plural_form(
                        doc,
                        &cfg.source_locale,
                        base,
                        locale,
                        cat,
                        text,
                    ),
                    None => formats::xcstrings::set_translation(doc, local_key, locale, text),
                };
                if !ok {
                    anyhow::bail!("{local_key}: not in the catalog");
                }
                *dirty = true;
            }
            Loaded::Android { template, locales } => {
                let entry = match locales.get_mut(locale) {
                    Some(e) => e,
                    None => {
                        let path = root.join(project::locale_file(template, locale));
                        let doc = if path.exists() {
                            formats::android::parse(&std::fs::read_to_string(&path)?)?
                        } else {
                            formats::android::parse(&formats::android::empty_file())?
                        };
                        locales
                            .entry(locale.to_string())
                            .or_insert((path, doc, false))
                    }
                };
                match plural {
                    Some((base, cat)) => entry.1.set_plural_item(base, cat, text),
                    None => match entry.1.index_of(local_key) {
                        Some(i) => entry.1.set_text(i, 0, text),
                        None => entry.1.insert_string(local_key, text),
                    },
                }
                entry.2 = true;
            }
            Loaded::Json {
                template, locales, ..
            } => {
                let entry = match locales.get_mut(locale) {
                    Some(e) => e,
                    None => {
                        let path = root.join(project::locale_file(template, locale));
                        let existing = if path.exists() {
                            Some(
                                serde_json::from_str(&std::fs::read_to_string(&path)?)
                                    .context("existing locale JSON")?,
                            )
                        } else {
                            None
                        };
                        locales.entry(locale.to_string()).or_insert(JsonLocale {
                            path,
                            existing,
                            values: BTreeMap::new(),
                            dirty: false,
                        })
                    }
                };
                entry.values.insert(local_key.to_string(), text.to_string());
                entry.dirty = true;
            }
            Loaded::Arb {
                template, locales, ..
            } => {
                let entry = match locales.get_mut(locale) {
                    Some(e) => e,
                    None => {
                        let path = root.join(project::locale_file(template, locale));
                        let existing = if path.exists() {
                            Some(std::fs::read_to_string(&path)?)
                        } else {
                            None
                        };
                        locales.entry(locale.to_string()).or_insert(ArbLocale {
                            path,
                            locale: locale.to_string(),
                            existing,
                            values: BTreeMap::new(),
                            dirty: false,
                        })
                    }
                };
                entry.values.insert(local_key.to_string(), text.to_string());
                entry.dirty = true;
            }
            Loaded::PerLocale {
                format,
                source_text,
                source_units,
                template,
                locales,
            } => {
                let entry = match locales.get_mut(locale) {
                    Some(e) => e,
                    None => {
                        let path = root.join(project::locale_file(template, locale));
                        let existing = if path.exists() {
                            Some(std::fs::read_to_string(&path)?)
                        } else {
                            None
                        };
                        let doc = match format {
                            Format::Po => {
                                let src = formats::po::parse(source_text)?;
                                PerLocaleDoc::Po(formats::po::parse(&existing.unwrap_or_else(
                                    || formats::po::new_locale_file(&src, locale),
                                ))?)
                            }
                            _ => PerLocaleDoc::Resx(formats::resx::parse(
                                &existing
                                    .unwrap_or_else(|| formats::resx::new_locale_file(source_text)),
                            )?),
                        };
                        locales
                            .entry(locale.to_string())
                            .or_insert((path, doc, false))
                    }
                };
                match &mut entry.1 {
                    PerLocaleDoc::Po(doc) if plural.is_some() => {
                        let (base, cat) = plural.expect("checked");
                        if doc.index_of(base).is_none() {
                            let src = formats::po::parse(source_text)?;
                            let i = src
                                .index_of(base)
                                .with_context(|| format!("{base}: not in the source .po"))?;
                            let e = &src.entries[i];
                            doc.insert_plural(
                                e.ctxt.as_deref(),
                                &e.msgid,
                                e.plural.as_deref().unwrap_or(&e.msgid),
                                e.comment.as_deref(),
                            );
                        }
                        if !doc.set_plural_category(base, locale, cat, text) {
                            anyhow::bail!(
                                "{base}: cannot place plural form `{cat}` for {locale} (check the file's Plural-Forms header)"
                            );
                        }
                    }
                    PerLocaleDoc::Po(doc) => match doc.index_of(local_key) {
                        Some(i) => doc.set_msgstr(i, text),
                        None => {
                            let (ctxt, msgid) = match local_key.split_once('\u{4}') {
                                Some((c, m)) => (Some(c), m),
                                None => (None, local_key),
                            };
                            let comment = source_units
                                .iter()
                                .find(|u| u.key == local_key)
                                .and_then(|u| u.comment.clone());
                            doc.insert(ctxt, msgid, text, comment.as_deref());
                        }
                    },
                    PerLocaleDoc::Resx(doc) => match doc.index_of(local_key) {
                        Some(i) => doc.set_text(i, text),
                        None => doc.insert(local_key, text),
                    },
                }
                entry.2 = true;
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        for f in &mut self.files {
            match f {
                Loaded::Xcstrings { path, doc, dirty } if *dirty => {
                    write_atomic(path, &formats::xcstrings::serialize(doc))?;
                    *dirty = false;
                }
                Loaded::Android { locales, .. } => {
                    for (path, doc, dirty) in locales.values_mut() {
                        if *dirty {
                            write_atomic(path, &formats::android::serialize(doc))?;
                            *dirty = false;
                        }
                    }
                }
                Loaded::Json {
                    source,
                    style,
                    locales,
                    ..
                } => {
                    for l in locales.values_mut() {
                        if l.dirty {
                            let tree = formats::json::build_locale_tree(
                                source,
                                l.existing.as_ref(),
                                &l.values,
                            );
                            let style = existing_style(&l.path).unwrap_or_else(|| style.clone());
                            write_atomic(&l.path, &formats::json::render(&tree, &style))?;
                            l.dirty = false;
                        }
                    }
                }
                Loaded::PerLocale { locales, .. } => {
                    for (path, doc, dirty) in locales.values_mut() {
                        if *dirty {
                            let out = match doc {
                                PerLocaleDoc::Po(d) => formats::po::serialize(d),
                                PerLocaleDoc::Resx(d) => formats::resx::serialize(d),
                            };
                            write_atomic(path, &out)?;
                            *dirty = false;
                        }
                    }
                }
                Loaded::Arb {
                    source_text,
                    locales,
                    ..
                } => {
                    for l in locales.values_mut() {
                        if l.dirty {
                            let out = formats::arb::build_locale_file(
                                source_text,
                                l.existing.as_deref(),
                                &l.locale,
                                &l.values,
                            );
                            write_atomic(&l.path, &out)?;
                            l.existing = Some(out);
                            l.dirty = false;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn existing_style(path: &Path) -> Option<formats::json::Style> {
    std::fs::read_to_string(path)
        .ok()
        .map(|t| formats::json::Style::detect(&t))
}

/// Write via a temp file + rename so a crash never leaves a half-written locale file.
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!(
        "{}.polygo-tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}
