//! `heelonbackup backup`

use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct BackupArgs {
    /// Files or folders to back up instead of the configured sources
    pub paths: Vec<PathBuf>,

    /// Scan and check the connection, but transfer nothing
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// With --dry-run: print every file that would be backed up
    #[arg(short, long, requires = "dry_run")]
    pub list: bool,

    /// Number of parallel SMB sessions for this backup (overrides backup.workers)
    #[arg(short = 'w', long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..=16))]
    pub workers: Option<u16>,

    /// Maximum file size in bytes, 0 = unlimited (overrides backup.max_file_size)
    #[arg(long, value_name = "BYTES", value_parser = clap::value_parser!(u64))]
    pub max_file_size: Option<u64>,

    /// Extra exclude pattern (glob), appended to backup.excludes; repeatable
    #[arg(short = 'e', long, value_name = "PATTERN")]
    pub exclude: Vec<String>,
}

impl From<BackupArgs> for service::backup::BackupRequest {
    fn from(args: BackupArgs) -> Self {
        Self {
            paths: args.paths,
            dry_run: args.dry_run,
            list: args.list,
            workers: args.workers.map(usize::from),
            max_file_size: args.max_file_size,
            excludes: (!args.exclude.is_empty()).then_some(args.exclude),
        }
    }
}

/// The CLI only parses the arguments; the orchestration lives in `service::backup`
pub async fn run(config: &Config, args: BackupArgs) -> Result<Exit> {
    service::backup::run(config, args.into()).await
}
