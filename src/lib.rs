//! polygo — local-first, git-native localization.
//!
//! The library exposes format parsers/serializers (byte-stable round-trips are a
//! hard requirement), the unit model, the config and lockfile, and — later —
//! providers and validators.

pub mod check;
pub mod config;
pub mod core;
pub mod engine;
pub mod formats;
pub mod glossary;
pub mod init;
pub mod lockfile;
pub mod project;
pub mod provider;
