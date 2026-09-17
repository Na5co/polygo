//! polygo — local-first, git-native localization.
//!
//! The library exposes format parsers/serializers (byte-stable round-trips are a
//! hard requirement), the unit model, the config and lockfile, and — later —
//! providers and validators.

pub mod config;
pub mod core;
pub mod formats;
pub mod lockfile;
pub mod project;
