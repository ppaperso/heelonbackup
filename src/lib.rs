//! HeelonBackup - reliable backup library for Linux to Synology NAS via SMB
//!
//! - file-level backups stored as plain files, with optional age archives of sensitive sources
//! - SHA-256 checksum of every file, recorded in a manifest
//! - progress, ETA, clear final status and restore dry-run

pub mod backup;
pub mod cli;
pub mod config;
pub(crate) mod console;
pub mod error;
pub(crate) mod service;
pub mod storage;
pub mod ui;
