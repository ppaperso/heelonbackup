//! `heelonbackup restore`

use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct RestoreArgs {
    /// Backup name (YYYYMMDD_HHMMSS) [default: latest]
    pub backup: Option<String>,

    /// Restore under this folder instead of the original locations
    #[arg(short, long, value_name = "DIR")]
    pub target: Option<PathBuf>,

    /// Only restore this file or folder (original absolute path; repeatable)
    #[arg(short, long = "path", value_name = "PATH")]
    pub paths: Vec<String>,

    /// Check everything (NAS content, conflicts, free space, permissions) without writing anything
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Replace existing files that differ from the backup
    #[arg(long)]
    pub overwrite: bool,

    /// Print the action planned for every file
    #[arg(short, long)]
    pub list: bool,
}

impl From<RestoreArgs> for service::restore::RestoreRequest {
    fn from(args: RestoreArgs) -> Self {
        Self {
            backup: args.backup,
            target: args.target,
            paths: args.paths,
            dry_run: args.dry_run,
            overwrite: args.overwrite,
            list: args.list,
        }
    }
}

/// The CLI only parses the arguments; the planning and transfer live in `service::restore`
pub async fn run(config: &Config, args: RestoreArgs) -> Result<Exit> {
    service::restore::run(config, args.into()).await
}
