//! Previewed local history cleanup; never contacts the NAS or deletes backup data.

use super::Exit;
use crate::error::Result;
use crate::storage::LocalHistory;
use crate::ui;
use std::path::PathBuf;

/// Snapshot to confirm before removing only the listed local summaries.
pub(crate) struct Cleanup {
    history: LocalHistory,
    files: Vec<PathBuf>,
}

impl Cleanup {
    pub(crate) fn preview() -> Result<Self> {
        Self::from_history(LocalHistory::open())
    }

    #[cfg(test)]
    pub(crate) fn at(dir: PathBuf) -> Result<Self> {
        Self::from_history(LocalHistory::at(dir))
    }

    fn from_history(history: LocalHistory) -> Result<Self> {
        let files = history.cleanup_candidates()?;
        ui::say!(
            "Local history cleanup: {} summary file(s). No NAS data or configuration will be deleted.",
            files.len()
        );
        for path in &files {
            ui::say!("  {}", path.display());
        }
        Ok(Self { history, files })
    }

    pub(crate) fn count(&self) -> usize {
        self.files.len()
    }

    pub(crate) fn execute(self) -> Result<Exit> {
        self.history.clear_previewed(&self.files)?;
        ui::say!(
            "Removed {} local history summary file(s). NAS backups are unchanged.",
            self.files.len()
        );
        Ok(Exit::Success)
    }
}
