//! Run every validator over a project and collect findings.

use crate::check::code::Code;
use crate::check::report::Cx;
pub use crate::check::report::{Finding, Report};
use crate::config::Config;
use crate::project;
use anyhow::{Context, Result};
use std::path::Path;

pub struct Options {
    pub locales: Option<Vec<String>>,
    pub length_ratio: f64,
}

pub fn run(root: &Path, cfg: &Config, opts: &Options) -> Result<Report> {
    let locales: Vec<String> = opts
        .locales
        .clone()
        .unwrap_or_else(|| cfg.target_locales.clone());
    // A file that does not parse comes first: reading the project would either fail on it
    // or, worse, quietly carry on without it.
    let broken = crate::check::formats::syntax::scan(root, cfg)?;
    if !broken.is_empty() {
        let mut cx = Cx::new(root, cfg, locales.clone(), Default::default())?;
        if crate::check::formats::syntax::check(&mut cx, &broken) {
            // The rest of the check needs the file polygo cannot read; say so and stop.
            return Ok(cx.report);
        }
    }
    let units = project::load_units(root, cfg)?;
    // `polygo:skip` and `polygo:ignore=…` from developer comments, for the checks that
    // read files directly (units already exclude skipped keys).
    let (directive_skipped, directive_ignores) = project::directive_rules(root, cfg)?;
    let mut cx = Cx::new(root, cfg, locales, directive_skipped.into_iter().collect())?;
    crate::check::formats::syntax::check(&mut cx, &broken);
    let lock = crate::lockfile::Lock::load(&root.join(crate::lockfile::FILE_NAME))?;
    let glossary = crate::glossary::load(root, cfg)?;

    for locale in cx.locales.clone() {
        let total = units.iter().filter(|u| u.applies_to(&locale)).count();
        let done = units
            .iter()
            .filter(|u| u.applies_to(&locale) && u.translations.contains_key(&locale))
            .count();
        cx.report.coverage.insert(locale, (done, total));
    }

    crate::check::units::check(&mut cx, &units, &lock, &glossary, opts.length_ratio);
    crate::check::consistency::check(&mut cx, &units);
    crate::check::formats::check(&mut cx)?;

    let mut report = cx.report;
    // `polygo:ignore=code` in a comment and `[keys] ignore` in polygo.toml: quiet, per key
    // and code, without hiding the key from everything the way `skip` does.
    let ignore_globs: Vec<(globset::GlobSet, Vec<String>)> = cfg
        .keys
        .ignore
        .iter()
        .map(|(g, codes)| {
            let mut b = globset::GlobSetBuilder::new();
            b.add(
                globset::Glob::new(g)
                    .with_context(|| format!("[keys] ignore: bad glob {g:?} in polygo.toml"))?,
            );
            Ok((b.build()?, codes.clone()))
        })
        .collect::<Result<_>>()?;
    if !ignore_globs.is_empty() || !directive_ignores.is_empty() {
        report.retain(|f| {
            let (idx, local) = project::split_key(cfg, &f.key);
            let base = local.split('#').next().unwrap_or(local);
            let code: &str = f.code.as_str();
            let by_comment = directive_ignores
                .get(&(idx, base.to_string()))
                .is_some_and(|codes| codes.iter().any(|c| c == code));
            let path = cfg.files[idx].path.display().to_string().replace('\\', "/");
            let by_glob = ignore_globs.iter().any(|(set, codes)| {
                codes.iter().any(|c| c == code)
                    && (set.is_match(base) || set.is_match(format!("{path}:{base}")))
            });
            !(by_comment || by_glob)
        });
    }
    report.findings.sort_by(|a, b| {
        (&a.file, &a.locale, &a.key, a.code.as_str()).cmp(&(
            &b.file,
            &b.locale,
            &b.key,
            b.code.as_str(),
        ))
    });
    Ok(report)
}

/// Everything `check` can report, for `--explain`.
pub fn codes() -> &'static [Code] {
    &Code::ALL
}
