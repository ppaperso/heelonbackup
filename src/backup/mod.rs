//! Local side of the backup: discovering and hashing files

pub mod hasher;
pub mod scanner;

pub use hasher::hash_file;
pub use scanner::{Excludes, ScanReport, ScannedFile, scan};
