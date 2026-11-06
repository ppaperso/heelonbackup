//! `heelonbackup config`

use crate::error::Result;
use crate::service::{self, Exit};
use clap::Subcommand;
use std::path::Path;

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Create the configuration file (interactive when run in a terminal)
    Init {
        /// Replace an existing configuration
        #[arg(long)]
        force: bool,
    },
    /// Print the configuration (password hidden)
    Show,
    /// Open the configuration in $VISUAL / $EDITOR, then validate it
    Edit,
    /// Validate the configuration file and resolve the configured sources (no network access)
    Check,
    /// List the configured sources (found on disk or missing) and the exclude patterns
    Sources,
    /// Print the path of the configuration file
    Path,
}

impl From<ConfigAction> for service::config::Action {
    fn from(action: ConfigAction) -> Self {
        match action {
            ConfigAction::Init { force } => Self::Init { force },
            ConfigAction::Show => Self::Show,
            ConfigAction::Edit => Self::Edit,
            ConfigAction::Check => Self::Check,
            ConfigAction::Sources => Self::Sources,
            ConfigAction::Path => Self::Path,
        }
    }
}

/// The CLI only parses the arguments; the actions live in `service::config`
pub fn run(path: &Path, action: Option<ConfigAction>) -> Result<Exit> {
    service::config::run(path, action.map(Into::into))
}
