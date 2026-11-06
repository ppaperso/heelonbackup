//! Console flow: verify a backup stored on the NAS

use super::{Strings, confirm, select, validate};
use crate::config::Config;
use crate::error::{BackupError, Result};
use crate::service::{self, Exit};
use std::path::Path;

/// Index 0 = latest usable backup; otherwise the picked name must be a valid backup name
fn picked_backup_name(index: usize, items: &[&str]) -> Result<Option<String>> {
    if index == 0 {
        return Ok(None);
    }
    let name = items
        .get(index)
        .ok_or_else(|| BackupError::BackupNotFound("no such backup in the list".into()))?;
    validate::validate_backup_name(name)?;
    Ok(Some(name.to_string()))
}

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
    let name = picked_backup_name(index, &items)?;
    let deep = matches!(
        confirm(s.verify_deep_prompt, s.yes, s.no, false).await?,
        Some(true)
    );
    service::verify::run(&config, name.as_deref(), deep).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_picking() {
        let items = ["Latest usable backup", "20261010_094120", "20261010_100512"];
        assert_eq!(picked_backup_name(0, &items).unwrap(), None);
        assert_eq!(
            picked_backup_name(1, &items).unwrap(),
            Some("20261010_094120".to_string())
        );
        assert!(picked_backup_name(9, &items).is_err());
        let bad = ["Latest usable backup", "not-a-name"];
        assert!(picked_backup_name(1, &bad).is_err());
    }
}
