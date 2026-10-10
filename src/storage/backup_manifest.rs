//! Backup manifest and local backup history

use crate::config::Environment;
use crate::error::{BackupError, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::time::Duration;

pub const MANIFEST_VERSION: u32 = 2;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const SUMMARY_FILE: &str = "summary.json";
/// Reason prefix for files that were not attempted (interruption, aborted transfer)
pub const NOT_TRANSFERRED: &str = "not transferred";
/// Number of failed/skipped entries kept in summaries (the full lists stay in the manifest)
const SUMMARY_ISSUES: usize = 100;

/// Status of a backup operation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupStatus {
    InProgress,
    Completed,
    CompletedWithErrors,
    Failed,
}

impl BackupStatus {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Completed => "✅",
            Self::CompletedWithErrors => "⚠️",
            Self::Failed => "❌",
            Self::InProgress => "⏳",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::CompletedWithErrors => "completed with errors",
            Self::Failed => "failed",
            Self::InProgress => "in progress (or interrupted)",
        }
    }

    /// True if the backup contains usable data
    pub fn is_usable(self) -> bool {
        matches!(self, Self::Completed | Self::CompletedWithErrors)
    }
}

/// Information about a single backed-up file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileInfo {
    pub size: u64,
    pub sha256: String,
    /// Modification time (seconds since the Unix epoch)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<i64>,
    /// Unix permission bits
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

/// A file that was skipped or could not be transferred
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileIssue {
    pub path: String,
    pub reason: String,
}

/// Everything known about one backup. Stored on the NAS as `<backup>/manifest.json`
/// (with `summary.json`, the same document without the file lists).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: u32,
    pub backup_id: String,
    /// Directory name of the backup (`YYYYMMDD_HHMMSS`)
    pub name: String,
    /// Directory of the backup, relative to the share root
    pub backup_path: String,
    /// `//host/share`
    pub server: String,
    pub hostname: String,
    pub sources: Vec<String>,
    pub status: BackupStatus,
    pub started_at: DateTime<Local>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Local>>,
    pub file_count: u64,
    pub total_size: u64,
    #[serde(default)]
    pub failed_count: u64,
    #[serde(default)]
    pub skipped_count: u64,
    #[serde(default)]
    pub failed: Vec<FileIssue>,
    #[serde(default)]
    pub skipped: Vec<FileIssue>,
    /// Absolute local path -> file information
    #[serde(default)]
    pub files: BTreeMap<String, FileInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Backup of specific paths given on the command line (not the configured sources)
    #[serde(default)]
    pub partial: bool,
    /// The presence and size of every saved file was checked on the NAS after the upload
    #[serde(default)]
    pub verified: bool,
}

/// Characters that are valid in Linux file names but rejected by SMB servers are mapped to
/// the private-use characters used by macOS and Samba's `vfs_catia` (e.g. `:` -> U+F022),
/// so that names such as GNOME screenshots (`Screenshot 12:34:56.png`) can be backed up.
/// The manifest always keeps the original name.
pub fn encode_remote_name(path: &str) -> String {
    path.chars()
        .map(|c| match c {
            '*' => '\u{F021}',
            ':' => '\u{F022}',
            '<' => '\u{F023}',
            '>' => '\u{F024}',
            '?' => '\u{F025}',
            '|' => '\u{F027}',
            c => c,
        })
        .collect()
}

/// Name of a new backup directory
pub fn new_backup_name(now: DateTime<Local>) -> String {
    now.format("%Y%m%d_%H%M%S").to_string()
}

/// True for directory names produced by [`new_backup_name`]
pub fn is_backup_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 15
        && b[8] == b'_'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 8 || c.is_ascii_digit())
}

impl BackupManifest {
    pub fn new(name: String, backup_path: String, server: String, sources: Vec<String>) -> Self {
        Self {
            version: MANIFEST_VERSION,
            backup_id: uuid::Uuid::new_v4().to_string(),
            name,
            backup_path,
            server,
            hostname: nix::unistd::gethostname()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_default(),
            sources,
            status: BackupStatus::InProgress,
            started_at: Local::now(),
            finished_at: None,
            file_count: 0,
            total_size: 0,
            failed_count: 0,
            skipped_count: 0,
            failed: Vec::new(),
            skipped: Vec::new(),
            files: BTreeMap::new(),
            notes: None,
            partial: false,
            verified: false,
        }
    }

    /// Remote directory holding the files
    pub fn data_dir(&self) -> String {
        format!("{}/data", self.backup_path)
    }

    /// Remote path of a file from its manifest key (absolute local path)
    pub fn remote_path(&self, key: &str) -> String {
        format!(
            "{}/{}",
            self.data_dir(),
            encode_remote_name(key.trim_start_matches('/'))
        )
    }

    pub fn add_file(&mut self, key: String, info: FileInfo) {
        self.file_count += 1;
        self.total_size += info.size;
        self.files.insert(key, info);
    }

    pub fn add_failure(&mut self, path: String, reason: String) {
        self.failed_count += 1;
        self.failed.push(FileIssue { path, reason });
    }

    /// Move a file from the backed-up list to the failures (e.g. failed verification)
    pub fn reject(&mut self, key: &str, reason: String) {
        if let Some(info) = self.files.remove(key) {
            self.file_count -= 1;
            self.total_size -= info.size;
            self.add_failure(key.to_string(), reason);
        }
    }

    /// Set the final status from the transfer results
    pub fn finish(&mut self) {
        self.finished_at = Some(Local::now());
        self.status = match (self.failed_count, self.file_count) {
            (0, _) => BackupStatus::Completed,
            (_, 0) => BackupStatus::Failed,
            _ => BackupStatus::CompletedWithErrors,
        };
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.status = BackupStatus::Failed;
        self.finished_at = Some(Local::now());
        self.notes = Some(reason.into());
    }

    pub fn duration(&self) -> Option<Duration> {
        (self.finished_at? - self.started_at).to_std().ok()
    }

    /// Average throughput in bytes per second
    pub fn throughput(&self) -> Option<f64> {
        let secs = self.duration()?.as_secs_f64();
        (secs > 0.0).then(|| self.total_size as f64 / secs)
    }

    /// Same document without the file list, with truncated issue lists
    pub fn summary(&self) -> Self {
        Self {
            files: BTreeMap::new(),
            failed: self.failed.iter().take(SUMMARY_ISSUES).cloned().collect(),
            skipped: self.skipped.iter().take(SUMMARY_ISSUES).cloned().collect(),
            ..self.clone()
        }
    }
}

/// Summaries of the backups made from this computer
/// (`~/.local/share/heelonbackup/history`, or `.heelonbackup/history` in development)
pub struct LocalHistory {
    dir: PathBuf,
}

impl LocalHistory {
    pub fn open() -> Self {
        Self {
            dir: Environment::detect().history_dir(),
        }
    }

    #[cfg(test)]
    fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn save(&self, manifest: &BackupManifest) -> Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)
            .map_err(|e| BackupError::io(format!("cannot create {}", self.dir.display()), e))?;
        let path = self.dir.join(format!("{}.json", manifest.name));
        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_vec_pretty(&manifest.summary())?;
        fs::write(&tmp, content)
            .and_then(|()| fs::rename(&tmp, &path))
            .map_err(|e| BackupError::io(format!("cannot write {}", path.display()), e))
    }

    pub fn remove(&self, name: &str) {
        let _ = fs::remove_file(self.dir.join(format!("{name}.json")));
    }

    /// All summaries, newest first (unreadable files are ignored)
    pub fn load_all(&self) -> Vec<BackupManifest> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut manifests: Vec<BackupManifest> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|e| fs::read(e.path()).ok())
            .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
            .collect();
        manifests.sort_by_key(|m| std::cmp::Reverse(m.started_at));
        manifests
    }

    /// Average throughput of the most recent meaningful backup, used for time estimates
    pub fn measured_rate(&self) -> Option<f64> {
        self.load_all()
            .into_iter()
            .filter(|m| m.status.is_usable() && m.total_size >= 8 * 1024 * 1024)
            .filter(|m| m.duration().is_some_and(|d| d.as_secs() >= 1))
            .find_map(|m| m.throughput())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(size: u64) -> FileInfo {
        FileInfo {
            size,
            sha256: "00".into(),
            mtime: None,
            mode: None,
        }
    }

    #[test]
    fn backup_names() {
        let name = new_backup_name(Local::now());
        assert!(is_backup_name(&name));
        assert!(!is_backup_name("2024_backup"));
        assert!(!is_backup_name("20241007-143022"));
        assert!(!is_backup_name("summary.json"));
    }

    #[test]
    fn status_from_results() {
        let mut m = BackupManifest::new("n".into(), "b/n".into(), "//h/s".into(), vec![]);
        m.add_file("/a".into(), info(10));
        m.finish();
        assert_eq!(m.status, BackupStatus::Completed);
        m.add_failure("/b".into(), "denied".into());
        m.finish();
        assert_eq!(m.status, BackupStatus::CompletedWithErrors);
        assert_eq!(m.total_size, 10);
        assert_eq!(m.remote_path("/home/u/x y"), "b/n/data/home/u/x y");
        assert_eq!(
            m.remote_path("/a/b 12:34?.png"),
            "b/n/data/a/b 12\u{F022}34\u{F025}.png"
        );

        let mut empty = BackupManifest::new("n".into(), "b/n".into(), "//h/s".into(), vec![]);
        empty.add_failure("/b".into(), "denied".into());
        empty.finish();
        assert_eq!(empty.status, BackupStatus::Failed);
    }

    #[test]
    fn history_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let history = LocalHistory::at(dir.path().join("history"));
        let mut m = BackupManifest::new(
            "20260101_120000".into(),
            "b/20260101_120000".into(),
            "//h/s".into(),
            vec![],
        );
        m.add_file("/a".into(), info(5));
        m.finish();
        history.save(&m).unwrap();

        let all = history.load_all();
        assert_eq!(all.len(), 1);
        assert!(
            all[0].files.is_empty(),
            "summaries must not contain the file list"
        );
        assert_eq!(all[0].total_size, 5);

        history.remove("20260101_120000");
        assert!(history.load_all().is_empty());
    }
}
