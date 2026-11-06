//! `heelonbackup status` (local history) and `heelonbackup list` (backups on the NAS)

use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};

/// The CLI only parses the arguments; the orchestration lives in `service::status`
pub fn status(all: bool, detailed: bool) -> Exit {
    service::status::status(all, detailed)
}

pub async fn list(config: &Config) -> Result<Exit> {
    service::status::list(config).await
}
