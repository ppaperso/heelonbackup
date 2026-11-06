//! Helpers shared by the CLI commands

use crate::config::Config;
use crate::error::Result;
use std::path::Path;
use tracing::warn;

pub fn load_config(path: &Path, verbose: bool) -> Result<Config> {
    let config = Config::load(path)?;
    super::init_logging(verbose, Some(&config.log_level));
    if let Some(warning) = Config::permission_warning(path, &config) {
        warn!("{warning}");
    }
    Ok(config)
}
