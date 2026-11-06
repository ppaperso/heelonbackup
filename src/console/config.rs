//! Console flow: configuration (show / check / path / edit)

use super::{Strings, select};
use crate::error::Result;
use crate::service::{self, Exit};
use crate::ui;
use std::path::Path;

/// Menu index -> configuration action
fn config_action(index: usize) -> service::config::Action {
    match index {
        0 => service::config::Action::Show,
        1 => service::config::Action::Check,
        2 => service::config::Action::Path,
        _ => service::config::Action::Edit,
    }
}

/// Configuration flow: delegates to the shared `service::config` actions.
/// The editor needs the real terminal: the dashboard steps aside while it runs.
pub(crate) async fn run(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let Some(index) = select(s.config_prompt, &s.config_options, 0).await? else {
        return Ok(Exit::Success); // Esc / Ctrl-C: back to the menu
    };
    let action = config_action(index);
    if action == service::config::Action::Edit {
        ui::say!("{}", s.editor_running);
        let path = config_path.to_path_buf();
        return super::prompt::suspended(move || service::config::run(&path, Some(action))).await?;
    }
    service::config::run(config_path, Some(action))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_action_mapping() {
        assert_eq!(config_action(0), service::config::Action::Show);
        assert_eq!(config_action(1), service::config::Action::Check);
        assert_eq!(config_action(2), service::config::Action::Path);
        assert_eq!(config_action(3), service::config::Action::Edit);
        assert_eq!(config_action(99), service::config::Action::Edit);
    }
}
