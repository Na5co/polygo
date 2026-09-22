//! polygo: local-first, git-native localization.
//!
//! The library exposes format parsers/serializers (byte-stable round-trips are a
//! hard requirement), the unit model, the config and lockfile, and: later :
//! providers and validators.

/// The version of polygo this build is: what the CLI prints, and what anything built on
/// the library (the bot) reports rather than a version of its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod audit;
pub mod check;
pub mod config;
pub mod context;
pub mod core;
pub mod coverage;
pub mod doctor;
pub mod engine;
pub mod extract;
pub mod formats;
pub mod glossary;
pub mod init;
pub mod lockfile;
pub mod memory;
pub mod models;
pub mod project;
pub mod provider;
pub mod pseudo;
pub mod remote;
pub mod review;
pub mod trace;
