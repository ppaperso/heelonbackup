//! `heelonbackup verify`

use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};

/// The CLI only parses the arguments; the orchestration lives in `service::verify`
pub async fn run(config: &Config, name: Option<&str>, deep: bool) -> Result<Exit> {
    service::verify::run(config, name, deep).await
}
