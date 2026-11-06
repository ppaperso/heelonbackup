//! Console flow: verify a backup stored on the NAS

use super::{Strings, confirm, select, validate};
use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};
use std::path::Path;

/// Verify flow: picks a backup from the NAS list, then delegates to the
/// same `service::verify::run` as `heelonbackup verify`.
pub(crate) async fn run(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let config = Config::load(config_path)?;
    let client = service::remote::connect(&config).await?;
    let base = config.remote_base()?;
    let summaries = service::remote::load_summaries(&client, &base).await?;
    if summaries.is_empty() {
        println!("{}", s.no_backup_on_nas);
        return Ok(Exit::Success);
    }
    let mut items: Vec<&str> = Vec::with_capacity(summaries.len() + 1);
    items.push(s.verify_latest);
    items.extend(summaries.iter().map(|(name, _)| name.as_str()));
    let Some(index) = select(s.verify_prompt, &items, 0).await? else {
        return Ok(Exit::Success); // Esc / Ctrl-C: back to the menu
    };
    let name = validate::picked_backup_name(index, &items)?;
    let deep = matches!(
        confirm(s.verify_deep_prompt, s.yes, s.no, false).await?,
        Some(true)
    );
    service::verify::run(&config, name.as_deref(), deep).await
}
