//! Localization file formats. Every format must round-trip byte-for-byte.

pub mod android;
pub mod arb;
pub mod json;
pub mod po;
pub mod resx;
pub mod strings;
pub mod stringsdict;
pub mod xcstrings;

/// Read a localization file as text. UTF-16 with a BOM (common for legacy `.strings`) is
/// decoded; everything else must be UTF-8.
pub fn read_text(path: &std::path::Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(strings::decode_file(&bytes)?.0)
}
