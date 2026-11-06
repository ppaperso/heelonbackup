//! Interactive console (simple TUI via dialoguer menus)
//!
//! Opened when `heelonbackup` runs without a subcommand. The console holds no
//! business logic: each flow collects the user's choices, then calls the same
//! `service::` orchestration as the CLI. Esc / q / Ctrl-C always leave cleanly
//! (never a panic); during a real transfer the CLI behaviour applies
//! (`BackupError::Interrupted` -> exit 130).

mod backup;
mod config;
mod restore;
mod status;
mod verify;

use crate::error::{BackupError, Result};
use crate::service::Exit;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use std::io::{ErrorKind, IsTerminal};

/// Main menu entries, in display order
const MENU: &[&str] = &[
    "Lancer un backup",
    "Statut",
    "Vérifier",
    "Restaurer",
    "Configuration",
    "Quitter",
];

/// Action behind a menu entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Backup,
    Status,
    Verify,
    Restore,
    Config,
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
        _ => Action::Quit,
    }
}

/// Main loop of the interactive console
pub(crate) async fn run() -> Result<Exit> {
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "The interactive console needs a terminal; pass a subcommand instead (see `heelonbackup --help`)."
        );
        return Ok(Exit::Failure);
    }
    loop {
        // The dialogue is blocking: build and run it on a dedicated thread so
        // Ctrl-C (the terminal layer restores the terminal, then raises SIGINT)
        // can interrupt it cleanly.
        let handle = tokio::task::spawn_blocking(|| {
            let theme = ColorfulTheme::default();
            Select::with_theme(&theme)
                .with_prompt("HeelonBackup — que souhaitez-vous faire ?")
                .items(MENU)
                .default(0)
                .interact_opt()
        });
        let choice = tokio::select! {
            joined = handle => joined.unwrap_or(Ok(None)),
            _ = tokio::signal::ctrl_c() => Ok(None),
        };
        match choice {
            Ok(Some(index)) => {
                match action_at(index) {
                    Action::Backup => backup::run().await?,
                    Action::Status => status::run().await?,
                    Action::Verify => verify::run().await?,
                    Action::Restore => restore::run().await?,
                    Action::Config => config::run().await?,
                    Action::Quit => return Ok(Exit::Success),
                };
            }
            // Esc / q / Ctrl-C at the main menu: clean quit
            Ok(None) => return Ok(Exit::Success),
            Err(dialoguer::Error::IO(e)) if e.kind() == ErrorKind::Interrupted => {
                return Ok(Exit::Success);
            }
            Err(dialoguer::Error::IO(e)) => {
                return Err(BackupError::io("console dialogue failed", e));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_actions_are_mapped_in_order() {
        assert_eq!(MENU.len(), 6);
        assert_eq!(action_at(0), Action::Backup);
        assert_eq!(action_at(1), Action::Status);
        assert_eq!(action_at(2), Action::Verify);
        assert_eq!(action_at(3), Action::Restore);
        assert_eq!(action_at(4), Action::Config);
        assert_eq!(action_at(5), Action::Quit);
        assert_eq!(action_at(usize::MAX), Action::Quit);
    }
}
