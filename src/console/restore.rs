//! Console flow: restore from a backup stored on the NAS — dry run first, always

use super::{Strings, confirm, input, select, validate};
use crate::config::Config;
use crate::error::Result;
use crate::service::Exit;
use crate::service::restore::{self, RestoreRequest};
use crate::ui;
use std::path::{Path, PathBuf};

/// Restore flow: picks a backup, a target and optional paths, runs a dry run
/// through the shared `service::restore::run`, and only executes the real
/// restore after an explicit confirmation.
pub(crate) async fn run(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let config = Config::load(config_path)?;
    if !super::ensure_password(s, &config).await? {
        return Ok(Exit::Success); // password dialog cancelled
    }
    let client = crate::service::remote::connect(&config).await?;
    let base = config.remote_base()?;
    let summaries = crate::service::remote::load_summaries(&client, &base).await?;
    if summaries.is_empty() {
        ui::say!("{}", s.no_backup_on_nas);
        return Ok(Exit::Success);
    }
    let mut items: Vec<&str> = Vec::with_capacity(summaries.len() + 1);
    items.push(s.restore_latest);
    items.extend(summaries.iter().map(|(name, _)| name.as_str()));
    let Some(index) = select(s.restore_prompt, &items, 0).await? else {
        return Ok(Exit::Success); // Esc / Ctrl-C: back to the menu
    };
    let backup = validate::picked_backup_name(index, &items)?;

    let Some(target_text) = input(s.restore_target_prompt, None).await? else {
        return Ok(Exit::Success);
    };
    let target: Option<PathBuf> = validate::parse_target(&target_text);

    let Some(paths_text) = input(s.restore_paths_prompt, None).await? else {
        return Ok(Exit::Success);
    };
    let paths = validate::parse_path_list(&paths_text);

    let overwrite = matches!(
        confirm(s.restore_overwrite_prompt, s.yes, s.no, false).await?,
        Some(true)
    );

    // dry run first, always: it prints the full plan without writing anything
    let request = RestoreRequest {
        backup,
        target,
        paths,
        dry_run: true,
        overwrite,
        list: true,
    };
    restore::run(&config, request.clone()).await?;

    // the real restore only runs after an explicit confirmation
    match confirm(s.restore_run_prompt, s.yes, s.no, false).await? {
        Some(true) => {
            let mut request = request;
            request.dry_run = false;
            request.list = false;
            restore::run(&config, request).await
        }
        _ => Ok(Exit::Success),
    }
}
