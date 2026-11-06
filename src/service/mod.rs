//! Orchestration shared by the front-ends (`cli/` and, later, `console/`).
//!
//! Neither front-end contains business logic: it collects the user's choices,
//! then calls the functions of this module. Everything that prints is grouped
//! in small dedicated display functions marked `TODO(console)`: in phase 3 the
//! display will be passed as a parameter of the `run()` functions so the
//! console can render its own output.

pub(crate) mod backup;
pub(crate) mod remote;
pub(crate) mod restore;
pub(crate) mod status;
pub(crate) mod verify;

use std::path::PathBuf;

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
