//! Format-independent model: a translatable unit is a key, its source-language
//! text, and whatever translations currently exist.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub key: String,
    pub source: String,
    /// Developer comment / context attached to the key, if the format has one.
    pub comment: Option<String>,
    /// locale → translated text (only locales that actually have a value).
    pub translations: BTreeMap<String, String>,
}

/// BLAKE3 hash of a string, hex-encoded, truncated to 16 bytes (32 hex chars):
/// plenty for change detection and short enough to keep the lockfile readable.
pub fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..32].to_string()
}
