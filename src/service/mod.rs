//! Orchestration shared by the front-ends (`cli/` and `console/`).
//!
//! Neither front-end contains business logic: it collects the user's choices,
//! then calls the functions of this module. Everything that prints is grouped
//! in small display functions and goes through `ui::say!`, so the CLI prints
//! it as-is and the console shows it in its terminal pane.

pub(crate) mod backup;
pub(crate) mod config;
pub(crate) mod history;
pub(crate) mod remote;
pub(crate) mod restore;
pub(crate) mod seal;
pub(crate) mod status;
pub(crate) mod verify;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

static INTERRUPT: Notify = Notify::const_new();
static INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Resolves when the user asks to stop the running transfer: Ctrl-C (SIGINT) in
/// the CLI, or [`request_interrupt`] from the console, whose raw-mode terminal turns
/// Ctrl-C into a key press instead of a signal. A request made before the transfer
/// starts (e.g. during the scan) is kept and stops it as soon as it begins.
pub(crate) async fn interrupted() {
    let notified = INTERRUPT.notified();
    tokio::pin!(notified);
    // register before checking the flag, so a request in between is not lost
    notified.as_mut().enable();
    if INTERRUPT_REQUESTED.swap(false, Ordering::SeqCst) {
        return;
    }
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = notified => { INTERRUPT_REQUESTED.store(false, Ordering::SeqCst); }
    }
}

/// Ask the running (or next) transfer to stop
pub(crate) fn request_interrupt() {
    INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
    INTERRUPT.notify_waiters();
}

/// Forget a pending request (called before each new console operation)
pub(crate) fn clear_interrupt() {
    INTERRUPT_REQUESTED.store(false, Ordering::SeqCst);
}

/// Result of an operation, mapped to a process exit code by the CLI front-end
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Success,
    /// Done, but with problems the user should look at
    Warnings,
    Failure,
    Interrupted,
}

impl From<Exit> for std::process::ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(match exit {
            Exit::Success => 0,
            Exit::Failure => 1,
            Exit::Warnings => 2,
            Exit::Interrupted => 130,
        })
    }
}

/// Expand a leading `~/`
pub fn expand_tilde(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if path == "~" => dirs::home_dir().unwrap_or_else(|| PathBuf::from(path)),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn interrupt_requests_are_kept_until_consumed_or_cleared() {
        let wait = || tokio::time::timeout(Duration::from_millis(50), interrupted());

        // a request made before the transfer starts stops it immediately, once
        request_interrupt();
        assert!(wait().await.is_ok());
        assert!(wait().await.is_err());

        // a cleared request is forgotten
        request_interrupt();
        clear_interrupt();
        assert!(wait().await.is_err());

        // a request made while waiting wakes the transfer
        let waiter = tokio::spawn(interrupted());
        tokio::time::sleep(Duration::from_millis(20)).await;
        request_interrupt();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), waiter)
                .await
                .is_ok()
        );
    }
}
