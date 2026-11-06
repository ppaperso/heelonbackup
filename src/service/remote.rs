//! SMB connection and remote manifest helpers shared by every operation

use crate::config::{Config, SmbTarget};
use crate::error::{BackupError, Result};
use crate::storage::backup_manifest::{MANIFEST_FILE, SUMMARY_FILE, is_backup_name};
use crate::storage::smb_client::{self, Reachability, quote};
use crate::storage::{BackupManifest, SmbClient};
use crate::ui;
use std::time::Duration;

/// How long a reachability check waits for the NAS
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Is the configured NAS reachable on its SMB port? (no password needed)
pub async fn reachability(config: &Config) -> Result<(SmbTarget, Reachability)> {
    let target = SmbTarget::parse(&config.smb.url)?;
    let state = smb_client::probe(&target, PROBE_TIMEOUT).await;
    Ok((target, state))
}

/// `host:port` actually probed by [`reachability`]
pub fn probed_address(target: &SmbTarget) -> String {
    format!(
        "{}:{}",
        target.host,
        target.port.unwrap_or(smb_client::SMB_PORT)
    )
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
