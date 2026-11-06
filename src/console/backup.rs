//! Console flow: start a backup — a dry run is always proposed before any real transfer

use super::{Strings, confirm, input, select, validate};
use crate::config::Config;
use crate::error::Result;
use crate::service::Exit;
use crate::service::backup::{self, BackupRequest};
use std::path::Path;

/// Menu index -> use the configured sources (index 0) or specific paths
fn use_configured_sources(index: usize) -> bool {
    index == 0
}

/// Backup flow: collects the choices, then calls the same `service::backup::run`
/// as `heelonbackup backup`. No real transfer happens without an explicit confirmation.
pub(crate) async fn run(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let config = Config::load(config_path)?;

    // 1. what to back up
    let Some(source) = select(s.backup_source_prompt, &s.backup_source_options, 0).await? else {
        return Ok(Exit::Success); // Esc / Ctrl-C: back to the menu
    };
    let paths = if use_configured_sources(source) {
        Vec::new()
    } else {
        let Some(text) = input(s.backup_paths_prompt, None).await? else {
            return Ok(Exit::Success);
        };
        // every given path must exist (InvalidPath otherwise)
        validate::existing_paths(&validate::parse_path_list(&text))?
    };

    // 2. a dry run is always proposed before any real transfer
    let run_dry = matches!(
        confirm(s.backup_dryrun_prompt, s.yes, s.no, true).await?,
        Some(true)
    );
    if run_dry {
        let list = matches!(
            confirm(s.backup_list_prompt, s.yes, s.no, false).await?,
            Some(true)
        );
        backup::run(
            &config,
            BackupRequest {
                paths: paths.clone(),
                dry_run: true,
                list,
                // technical overrides (workers, max_file_size) stay CLI-only: None = config values
                workers: None,
                max_file_size: None,
            },
        )
        .await?;
    }

    // 3. the real backup only runs after an explicit confirmation
    match confirm(s.backup_run_prompt, s.yes, s.no, false).await? {
        Some(true) => {
            backup::run(
                &config,
                BackupRequest {
                    paths,
                    dry_run: false,
                    list: false,
                    workers: None,
                    max_file_size: None,
                },
            )
            .await
        }
        _ => Ok(Exit::Success),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_mapping() {
        assert!(use_configured_sources(0));
        assert!(!use_configured_sources(1));
        assert!(!use_configured_sources(99));
    }
}
