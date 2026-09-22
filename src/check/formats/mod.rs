//! Checks that need the file's own structure rather than a unit: plural blocks, Xcode
//! state, Android escapes and arrays, `.stringsdict`, plus what every per-locale format
//! shares (duplicate keys, orphans, gettext's fuzzy flag).

pub mod android;
pub mod files;
pub mod json;
pub mod strings;
pub mod syntax;
pub mod xcstrings;

use crate::check::report::Cx;
use crate::config::Format;
use anyhow::Result;

pub fn check(cx: &mut Cx) -> Result<()> {
    let mut seen_dicts = std::collections::BTreeSet::new();
    for idx in 0..cx.cfg.files.len() {
        match cx.cfg.files[idx].format {
            Format::Xcstrings => xcstrings::check(cx, idx)?,
            Format::Android => android::check(cx, idx)?,
            Format::Json => json::check(cx, idx)?,
            Format::Strings => strings::check(cx, idx, &mut seen_dicts)?,
            Format::Arb | Format::Po | Format::Resx => {}
        }
        files::check(cx, idx)?;
    }
    Ok(())
}
