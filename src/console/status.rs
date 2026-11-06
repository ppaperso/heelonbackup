//! Console flows: status of the local backup history, backups stored on the NAS

use super::{Strings, confirm, select};
use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};
use std::path::Path;

/// Menu index -> (all, detailed) flags of `service::status::status`
fn status_view(index: usize) -> (bool, bool) {
    match index {
        0 => (false, false),
        1 => (false, true),
        _ => (true, true),
    }
}

/// Status flow: delegates to the shared `service::status` orchestration
pub(crate) async fn run(s: &'static Strings, _config_path: &Path) -> Result<Exit> {
    let Some(index) = select(s.status_prompt, &s.status_options, 0).await? else {
        return Ok(Exit::Success); // Esc / Ctrl-C: back to the menu
    };
    if index == 3 {
        let cleanup = service::history::Cleanup::preview()?;
        return clear_history(s, cleanup).await;
    }
    let (all, detailed) = status_view(index);
    Ok(service::status::status(all, detailed))
}

pub(crate) async fn clear_history(
    s: &'static Strings,
    cleanup: service::history::Cleanup,
) -> Result<Exit> {
    if cleanup.count() == 0 {
        return Ok(Exit::Success);
    }
    let title = format!("{} ({})", s.history_clear_prompt, cleanup.count());
    if confirm(&title, s.yes, s.no, false).await? == Some(true) {
        cleanup.execute()
    } else {
        Ok(Exit::Success)
    }
}

/// Backups on the NAS: same `service::status::list` as `heelonbackup list`
pub(crate) async fn run_list(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let config = Config::load(config_path)?;
    if !super::ensure_password(s, &config).await? {
        return Ok(Exit::Success); // password dialog cancelled
    }
    service::status::list(&config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_view_mapping() {
        assert_eq!(status_view(0), (false, false));
        assert_eq!(status_view(1), (false, true));
        assert_eq!(status_view(2), (true, true));
        assert_eq!(status_view(99), (true, true));
    }
}
