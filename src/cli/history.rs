//! `heelonbackup history clear`: local summaries only, explicit confirmation required.

use crate::error::Result;
use crate::service::{Exit, history::Cleanup};
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub action: HistoryAction,
}

#[derive(Subcommand, Debug)]
pub enum HistoryAction {
    /// Preview local summaries to delete; --yes executes, without touching the NAS
    Clear {
        #[arg(long, conflicts_with = "dry_run")]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run(args: HistoryArgs) -> Result<Exit> {
    let cleanup = Cleanup::preview()?;
    match args.action {
        HistoryAction::Clear { yes: true, .. } => cleanup.execute(),
        _ => Ok(Exit::Success),
    }
}
