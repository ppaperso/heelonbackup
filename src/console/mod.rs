//! Interactive console (simple TUI via dialoguer menus)
//!
//! Opened when `heelonbackup` runs without a subcommand. The console holds no
//! business logic: each flow collects the user's choices (bilingual EN/FR,
//! see `strings.rs`), then calls the same `service::` orchestration as the CLI.
//! Esc / q / Ctrl-C always leave cleanly (never a panic); during a real
//! transfer the service layer handles Ctrl-C itself (`Exit::Interrupted`).

mod backup;
mod config;
mod restore;
mod status;
mod strings;
mod verify;

pub(crate) use strings::{Lang, Strings};

use crate::config::Config;
use crate::error::{BackupError, Result};
use crate::service::Exit;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use std::io::{ErrorKind, IsTerminal};
use std::path::Path;
use tracing::warn;

/// Action behind a menu entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Backup,
    Status,
    Verify,
    Restore,
    Config,
    Language,
    Quit,
}

/// Menu index -> action (any index past the list is Quit)
fn action_at(index: usize) -> Action {
    match index {
        0 => Action::Backup,
        1 => Action::Status,
        2 => Action::Verify,
        3 => Action::Restore,
        4 => Action::Config,
        5 => Action::Language,
        _ => Action::Quit,
    }
}

/// Run a blocking dialogue on a dedicated thread so Ctrl-C (the terminal layer
/// restores the terminal, then raises SIGINT) interrupts it cleanly.
/// `Ok(None)` = Esc / q / Ctrl-C: the caller goes back without a panic.
pub(crate) async fn interact_opt<T, F>(f: F) -> Result<Option<T>>
where
    F: FnOnce() -> dialoguer::Result<Option<T>> + Send + 'static,
    T: Send + 'static,
{
    let handle = tokio::task::spawn_blocking(f);
    let res = tokio::select! {
        joined = handle => joined.unwrap_or(Ok(None)),
        _ = tokio::signal::ctrl_c() => Ok(None),
    };
    match res {
        Ok(value) => Ok(value),
        Err(dialoguer::Error::IO(e)) if e.kind() == ErrorKind::Interrupted => Ok(None),
        Err(dialoguer::Error::IO(e)) => Err(BackupError::io("console dialogue failed", e)),
    }
}

/// Select one item in a list
pub(crate) async fn select(prompt: &str, items: &[&str], default: usize) -> Result<Option<usize>> {
    let prompt = prompt.to_string();
    let items: Vec<String> = items.iter().map(|s| s.to_string()).collect();
    interact_opt(move || {
        let theme = ColorfulTheme::default();
        Select::with_theme(&theme)
            .with_prompt(prompt)
            .items(&items)
            .default(default)
            .interact_opt()
    })
    .await
}

/// Main loop of the interactive console
pub(crate) async fn run(config_path: &Path) -> Result<Exit> {
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "The interactive console needs a terminal; pass a subcommand instead (see `heelonbackup --help`)."
        );
        return Ok(Exit::Failure);
    }
    let config = Config::load(config_path).ok();
    if let Some(warning) = config
        .as_ref()
        .and_then(|c| Config::permission_warning(config_path, c))
    {
        warn!("{warning}");
    }
    let mut lang = config
        .as_ref()
        .map(|c| Lang::from_config(&c.language))
        .unwrap_or(Lang::En);
    loop {
        let s = lang.strings();
        let items = [
            s.menu_backup,
            s.menu_status,
            s.menu_verify,
            s.menu_restore,
            s.menu_config,
            s.menu_language,
            s.menu_quit,
        ];
        let Some(index) = select(s.menu_prompt, &items, 0).await? else {
            // Esc / q / Ctrl-C at the main menu: clean quit
            return Ok(Exit::Success);
        };
        let flow = match action_at(index) {
            Action::Backup => backup::run(s, config_path).await,
            Action::Status => status::run(s, config_path).await,
            Action::Verify => verify::run(s, config_path).await,
            Action::Restore => restore::run(s, config_path).await,
            Action::Config => config::run(s, config_path).await,
            Action::Language => {
                lang = lang.toggle();
                persist_language(config_path, lang)?;
                println!("{}", lang.strings().language_saved);
                continue;
            }
            Action::Quit => return Ok(Exit::Success),
        };
        match flow {
            // an interrupted transfer keeps the CLI semantics (exit 130)
            Ok(Exit::Interrupted) => return Ok(Exit::Interrupted),
            Ok(_) => {}
            Err(BackupError::Interrupted) => return Err(BackupError::Interrupted),
            // any other flow error is shown, then the menu comes back
            Err(e) => eprintln!("❌ Error: {e}"),
        }
    }
}

/// Save the console language in the configuration file
fn persist_language(path: &Path, lang: Lang) -> Result<()> {
    let mut config = Config::load(path).unwrap_or_default();
    config.language = lang.as_str().to_string();
    config.validate()?;
    config.save(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_actions_are_mapped_in_order() {
        assert_eq!(action_at(0), Action::Backup);
        assert_eq!(action_at(1), Action::Status);
        assert_eq!(action_at(2), Action::Verify);
        assert_eq!(action_at(3), Action::Restore);
        assert_eq!(action_at(4), Action::Config);
        assert_eq!(action_at(5), Action::Language);
        assert_eq!(action_at(6), Action::Quit);
        assert_eq!(action_at(usize::MAX), Action::Quit);
    }
}
