//! Helpers shared by the commands

use crate::config::{Config, SmbTarget};
use crate::error::{BackupError, Result};
use crate::storage::backup_manifest::{MANIFEST_FILE, SUMMARY_FILE, is_backup_name};
use crate::storage::smb_client::quote;
use crate::storage::{BackupManifest, SmbClient};
use crate::ui;
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Result of a command, mapped to the process exit code
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

pub fn load_config(path: &Path, verbose: bool) -> Result<Config> {
    let config = Config::load(path)?;
    super::init_logging(verbose, Some(&config.log_level));
    if let Some(warning) = Config::permission_warning(path, &config) {
        warn!("{warning}");
    }
    Ok(config)
}

/// Expand a leading `~/`
pub fn expand_tilde(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if path == "~" => dirs::home_dir().unwrap_or_else(|| PathBuf::from(path)),
        _ => PathBuf::from(path),
    }
}

/// Display form of a remote directory: `//host/share/dir`
pub fn remote_display(target: &SmbTarget, dir: &str) -> String {
    format!("{target}/{dir}")
}

/// Ask for the password if needed, then open and test the SMB connection
pub async fn connect(config: &Config) -> Result<SmbClient> {
    let password = config.resolve_password()?;
    let target = SmbTarget::parse(&config.smb.url)?;
    let spinner = ui::spinner(format!("Connecting to {target}..."));
    let result = SmbClient::connect(config, password).await;
    spinner.finish_and_clear();
    result
}

pub fn parse_manifest(bytes: &[u8]) -> Result<BackupManifest> {
    Ok(serde_json::from_slice(bytes)?)
}

/// Names of the backups on the NAS, newest first
pub async fn remote_backup_names(client: &SmbClient, base: &str) -> Result<Vec<String>> {
    let mut names: Vec<String> = client
        .list(base, false)
        .await?
        .into_iter()
        .filter(|e| e.is_dir)
        .filter_map(|e| e.path.rsplit('/').next().map(String::from))
        .filter(|n| is_backup_name(n))
        .collect();
    names.sort_unstable_by(|a, b| b.cmp(a));
    Ok(names)
}

/// Summaries of all backups on the NAS, newest first (`None`: no readable summary)
pub async fn load_summaries(
    client: &SmbClient,
    base: &str,
) -> Result<Vec<(String, Option<BackupManifest>)>> {
    let names = remote_backup_names(client, base).await?;
    let tmp = tempfile::tempdir().map_err(|e| BackupError::io("cannot create temporary dir", e))?;
    // all summaries in a single SMB session
    let mut script = String::new();
    for name in &names {
        let local = tmp.path().join(name);
        script.push_str(&format!(
            "get {} {}\n",
            quote(&format!("{base}/{name}/{SUMMARY_FILE}"))?,
            quote(local.to_str().unwrap_or_default())?
        ));
    }
    if !script.is_empty() {
        client.run_script(&script).await?;
    }
    Ok(names
        .into_iter()
        .map(|name| {
            let summary = std::fs::read(tmp.path().join(&name))
                .ok()
                .and_then(|b| parse_manifest(&b).ok());
            (name, summary)
        })
        .collect())
}

/// Load the full manifest of the given backup, or of the latest usable one
pub async fn resolve_backup(
    client: &SmbClient,
    config: &Config,
    name: Option<&str>,
) -> Result<BackupManifest> {
    let base = config.remote_base()?;
    let spinner = ui::spinner("Loading backup manifest...");
    let result = async {
        let name = match name {
            Some(name) if is_backup_name(name) => name.to_string(),
            Some(name) => {
                return Err(BackupError::BackupNotFound(format!(
                    "`{name}` is not a backup name (expected YYYYMMDD_HHMMSS, see `heelonbackup list`)"
                )));
            }
            None => latest_usable(client, &base).await?,
        };
        let bytes = client
            .download_bytes(&format!("{base}/{name}/{MANIFEST_FILE}"))
            .await
            .map_err(|e| match e {
                BackupError::BackupNotFound(_) => BackupError::BackupNotFound(format!(
                    "{name} (no {MANIFEST_FILE}; the backup is incomplete or does not exist)"
                )),
                e => e,
            })?;
        parse_manifest(&bytes)
    }
    .await;
    spinner.finish_and_clear();
    result
}

async fn latest_usable(client: &SmbClient, base: &str) -> Result<String> {
    for name in remote_backup_names(client, base).await? {
        match client
            .download_bytes(&format!("{base}/{name}/{SUMMARY_FILE}"))
            .await
        {
            Ok(bytes) => {
                if parse_manifest(&bytes).is_ok_and(|m| m.status.is_usable()) {
                    return Ok(name);
                }
            }
            Err(BackupError::BackupNotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Err(BackupError::BackupNotFound(
        "no usable backup on the NAS (run `heelonbackup list`)".into(),
    ))
}

/// `3 days ago`, `5 min ago`
pub fn age(date: DateTime<Local>) -> String {
    let secs = (Local::now() - date).num_seconds().max(0);
    match secs {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}

pub fn date(d: DateTime<Local>) -> String {
    d.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Print at most `max` items, then `... and N more`
pub fn print_limited<T>(items: &[T], max: usize, show: impl Fn(&T) -> String) {
    for item in items.iter().take(max) {
        println!("    • {}", show(item));
    }
    if items.len() > max {
        println!("    … and {} more", items.len() - max);
    }
}
