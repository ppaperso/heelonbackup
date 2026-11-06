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
}

impl From<BackupArgs> for service::backup::BackupRequest {
    fn from(args: BackupArgs) -> Self {
        Self {
            paths: args.paths,
            dry_run: args.dry_run,
            list: args.list,
        }
    }
}

/// The CLI only parses the arguments; the orchestration lives in `service::backup`
pub async fn run(config: &Config, args: BackupArgs) -> Result<Exit> {
    service::backup::run(config, args.into()).await
}
