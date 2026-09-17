//! polygo: local-first, git-native localization.
//!
//! The library exposes format parsers/serializers (byte-stable round-trips are a
//! hard requirement), the unit model, the config and lockfile, and: later :
//! providers and validators.

pub mod audit;
pub mod check;
pub mod config;
pub mod context;
pub mod core;
pub mod coverage;
pub mod doctor;
pub mod engine;
pub mod formats;
pub mod glossary;
pub mod init;
pub mod lockfile;
pub mod memory;
pub mod models;
pub mod project;
pub mod provider;
pub mod pseudo;
pub mod review;
