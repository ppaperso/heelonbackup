//! Backup orchestration: scan the sources, transfer the files to the NAS, apply retention
//!
//! Called identically by `heelonbackup backup` (cli) and the console backup flow.

use super::{Exit, expand_tilde, remote};
use crate::backup::{Excludes, ScanReport, scan};
use crate::config::{Config, SmbTarget};
use crate::error::{BackupError, Result};
use crate::storage::backup_manifest::{
    MANIFEST_FILE, NOT_TRANSFERRED, SUMMARY_FILE, new_backup_name,
};
use crate::storage::transfer::{Item, Mode, Outcome, Transfer};
use crate::storage::{BackupManifest, BackupStatus, FileInfo, LocalHistory, SmbClient};
use crate::ui;
use chrono::Local;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::warn;

/// Parameters of a backup run, collected by the CLI or the console
#[derive(Debug, Clone, Default)]
pub struct BackupRequest {
    /// Files or folders to back up instead of the configured sources
    pub paths: Vec<PathBuf>,
    /// Scan and check the connection, but transfer nothing
    pub dry_run: bool,
    /// With dry_run: print every file that would be backed up
    pub list: bool,
    /// Override `backup.workers` for this run (None = use the configuration)
    pub workers: Option<usize>,
    /// Override `backup.max_file_size` for this run (None = use the configuration)
    pub max_file_size: Option<u64>,
}

/// Effective configuration for a run: the request overrides win over the configuration
fn effective_config(config: &Config, request: &BackupRequest) -> Config {
    let mut config = config.clone();
    if let Some(workers) = request.workers {
        config.backup.workers = workers;
    }
    if let Some(max_file_size) = request.max_file_size {
        config.backup.max_file_size = max_file_size;
    }
    config
}

fn resolve_sources(config: &Config, request: &BackupRequest) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    if request.paths.is_empty() {
        for source in &config.backup.sources {
            let path = expand_tilde(source);
            match path.canonicalize() {
                Ok(p) => sources.push(p),
                Err(e) => warn!("Source {} ignored: {e}", path.display()),
            }
        }
        if sources.is_empty() {
            return Err(BackupError::InvalidPath(
                "none of the configured sources exists (backup.sources)".into(),
            ));
        }
    } else {
        for path in &request.paths {
            let canonical = path
                .canonicalize()
                .map_err(|e| BackupError::InvalidPath(format!("{}: {e}", path.display())))?;
            sources.push(canonical);
        }
    }
    sources.sort();
    sources.dedup();
    Ok(sources)
}

async fn scan_sources(config: &Config, sources: Vec<PathBuf>) -> Result<ScanReport> {
    let excludes = Excludes::new(&config.backup.excludes)?;
    let max_size = config.backup.max_file_size;
    let spinner = ui::spinner("Scanning files...");
    let pb = spinner.clone();
    let report = tokio::task::spawn_blocking(move || {
        scan(&sources, &excludes, max_size, &|files, bytes| {
            pb.set_message(format!(
                "Scanning files... {files} files, {}",
                ui::bytes(bytes)
            ));
        })
    })
    .await
    .expect("scan task panicked");
    spinner.finish_and_clear();
    Ok(report)
}

pub async fn run(config: &Config, request: BackupRequest) -> Result<Exit> {
    // request overrides (workers, max_file_size) apply to the whole run
    let config = effective_config(config, &request);
    let sources = resolve_sources(&config, &request)?;
    let started = Instant::now();
    let report = scan_sources(&config, sources.clone()).await?;
    let history = LocalHistory::open();
    let base = config.remote_base()?;
    let target = SmbTarget::parse(&config.smb.url)?;
    let name = new_backup_name(Local::now());

    let destination = remote::remote_display(&target, &format!("{base}/{name}"));
    print_scan_plan(
        &sources,
        &report,
        &destination,
        started.elapsed(),
        history.measured_rate(),
        request.dry_run,
    );

    if report.files.is_empty() {
        print_nothing_to_back_up();
        return Ok(if report.skipped.is_empty() {
            Exit::Success
        } else {
            Exit::Warnings
        });
    }

    if request.dry_run {
        print_dry_run(&report, request.list);
        return Ok(Exit::Success);
    }

    let client = remote::connect(&config).await?;
    print_connected(&target, config.smb.encrypt);

    let backup_path = format!("{base}/{name}");
    let mut manifest = BackupManifest::new(
        name,
        backup_path,
        target.to_string(),
        sources
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect(),
    );
    manifest.skipped_count = report.skipped.len() as u64;
    manifest.skipped = report.skipped.clone();

    // Backups of specific paths are partial: they must never trigger the deletion of full backups
    manifest.partial = !request.paths.is_empty();
    let exit = execute(&config, &client, &history, &mut manifest, &report).await;
    if let Err(e) = &exit {
        manifest.fail(e.to_string());
        save_history(&history, &manifest);
    }
    let exit = exit?;
    print_summary(&manifest, &target);
    Ok(exit)
}

fn save_history(history: &LocalHistory, manifest: &BackupManifest) {
    if let Err(e) = history.save(manifest) {
        warn!("Cannot update the local backup history: {e}");
    }
}

async fn execute(
    config: &Config,
    client: &SmbClient,
    history: &LocalHistory,
    manifest: &mut BackupManifest,
    report: &ScanReport,
) -> Result<Exit> {
    save_history(history, manifest);

    let items: Vec<Item> = report
        .files
        .iter()
        .map(|f| Item {
            local: f.key.clone(),
            remote: manifest.remote_path(&f.key),
            size: f.size,
        })
        .collect();

    let spinner = ui::spinner("Preparing remote folders...");
    let backup_dir = manifest.backup_path.clone();
    let parents = items
        .iter()
        .filter_map(|i| i.remote.rsplit_once('/').map(|(dir, _)| dir))
        .chain([backup_dir.as_str()]);
    let prepared = client.mkdirs(parents).await;
    let prepared = match prepared {
        Ok(()) => upload_json(client, manifest, true).await,
        Err(e) => Err(e),
    };
    spinner.finish_and_clear();
    prepared?;

    let pb = ui::transfer_bar(report.total_size, "Uploading");
    let transfer = Transfer::new(items, pb.clone());
    let interrupted = tokio::select! {
        () = transfer.run(client, Mode::Upload, config.backup.workers) => false,
        _ = tokio::signal::ctrl_c() => true,
    };
    pb.finish_and_clear();

    let fatal = transfer.fatal_error();
    let not_done = if interrupted {
        format!("{NOT_TRANSFERRED}: interrupted by user")
    } else {
        format!(
            "{NOT_TRANSFERRED}: {}",
            fatal.as_deref().unwrap_or("transfer aborted")
        )
    };
    for (file, outcome) in report.files.iter().zip(transfer.outcomes()) {
        match outcome {
            Some(Outcome::Done { sha256, size }) => manifest.add_file(
                file.key.clone(),
                FileInfo {
                    size,
                    sha256,
                    mtime: file.mtime,
                    mode: Some(file.mode),
                },
            ),
            Some(Outcome::Failed { reason, .. }) => manifest.add_failure(file.key.clone(), reason),
            None => manifest.add_failure(file.key.clone(), not_done.clone()),
        }
    }

    if config.backup.verify && !interrupted && manifest.file_count > 0 {
        let spinner = ui::spinner("Verifying files on the NAS...");
        let listing = client.list(&manifest.data_dir(), true).await;
        spinner.finish_and_clear();
        let remote: HashMap<String, u64> = listing?
            .into_iter()
            .filter(|e| !e.is_dir)
            .map(|e| (e.path, e.size))
            .collect();
        let rejected: Vec<(String, String)> = manifest
            .files
            .iter()
            .filter_map(|(key, info)| match remote.get(&manifest.remote_path(key)) {
                Some(&size) if size == info.size => None,
                Some(&size) => Some((
                    key.clone(),
                    format!(
                        "verification failed: {size} bytes on the NAS, {} expected",
                        info.size
                    ),
                )),
                None => Some((
                    key.clone(),
                    "verification failed: missing on the NAS".into(),
                )),
            })
            .collect();
        for (key, reason) in rejected {
            manifest.reject(&key, reason);
        }
        manifest.verified = true;
    }

    manifest.finish();
    if interrupted {
        manifest.status = BackupStatus::Failed;
        manifest.notes = Some("interrupted by user".into());
    } else if let Some(fatal) = fatal {
        manifest.status = BackupStatus::Failed;
        manifest.notes = Some(format!("transfer aborted: {fatal}"));
    }

    let spinner = ui::spinner("Saving manifest...");
    let saved = upload_json(client, manifest, false).await;
    spinner.finish_and_clear();
    if let Err(e) = saved {
        manifest.status = BackupStatus::Failed;
        manifest.notes = Some(format!("the manifest could not be saved on the NAS: {e}"));
    }
    save_history(history, manifest);

    if manifest.partial {
        // backup of specific paths: old backups are left untouched
    } else if manifest.status == BackupStatus::Completed {
        apply_retention(config, client, history, &manifest.name).await;
    } else if manifest.status == BackupStatus::CompletedWithErrors && config.storage.retention > 0 {
        print_retention_kept();
    }

    Ok(if interrupted {
        Exit::Interrupted
    } else {
        match manifest.status {
            BackupStatus::Completed => Exit::Success,
            BackupStatus::CompletedWithErrors => Exit::Warnings,
            _ => Exit::Failure,
        }
    })
}

/// Upload `summary.json` (and `manifest.json` unless `summary_only`)
async fn upload_json(
    client: &SmbClient,
    manifest: &BackupManifest,
    summary_only: bool,
) -> Result<()> {
    let dir = &manifest.backup_path;
    if !summary_only {
        let full = serde_json::to_vec_pretty(manifest)?;
        client
            .upload_bytes(&format!("{dir}/{MANIFEST_FILE}"), &full)
            .await?;
    }
    let summary = serde_json::to_vec_pretty(&manifest.summary())?;
    client
        .upload_bytes(&format!("{dir}/{SUMMARY_FILE}"), &summary)
        .await
}

/// Keep the newest `storage.retention` complete backups (the current one included).
/// Failed, interrupted and partial backups older than the current one are removed as well,
/// unless they are newer than the oldest complete backup that is kept.
async fn apply_retention(
    config: &Config,
    client: &SmbClient,
    history: &LocalHistory,
    current: &str,
) {
    let keep = config.storage.retention;
    if keep == 0 {
        return;
    }
    let Ok(base) = config.remote_base() else {
        return;
    };
    let backups = match remote::load_summaries(client, &base).await {
        Ok(backups) => backups,
        Err(e) => {
            warn!("Retention skipped: {e}");
            return;
        }
    };
    let to_delete = retention_candidates(&backups, current, keep);
    for name in to_delete {
        match client.deltree(&format!("{base}/{name}")).await {
            Ok(()) => {
                history.remove(name);
                print_removed(name, keep);
            }
            Err(e) => warn!("Cannot remove old backup {name}: {e}"),
        }
    }
}

/// `backups` must be sorted newest first
fn retention_candidates<'a>(
    backups: &'a [(String, Option<BackupManifest>)],
    current: &str,
    keep: usize,
) -> Vec<&'a str> {
    let mut kept_full = 1; // the current backup
    let mut to_delete = Vec::new();
    for (name, summary) in backups.iter().filter(|(n, _)| n.as_str() < current) {
        let full = summary
            .as_ref()
            .is_some_and(|s| s.status.is_usable() && !s.partial);
        let partial_ok = summary
            .as_ref()
            .is_some_and(|s| s.status.is_usable() && s.partial);
        if full && kept_full < keep {
            kept_full += 1;
        } else if partial_ok && kept_full < keep {
            // partial backups are kept while they are newer than the oldest kept full backup
        } else {
            to_delete.push(name.as_str());
        }
    }
    to_delete
}

// ---- Display ----
// TODO(console): all the printing of this module is grouped in the small
// functions below. In phase 3 the display will be passed as a parameter of
// run() so the console can render its own output; until then they keep the
// exact CLI output.

/// The scanned plan: sources, file count, destination, estimate
// TODO(console)
fn print_scan_plan(
    sources: &[PathBuf],
    report: &ScanReport,
    destination: &str,
    scanned_in: Duration,
    rate: Option<f64>,
    dry_run: bool,
) {
    println!("📦 Backup plan{}", if dry_run { " (dry run)" } else { "" });
    println!("{}", ui::RULE);
    for source in sources {
        println!("  Source:          {}", source.display());
    }
    println!(
        "  Files:           {} ({})  [scanned in {}]",
        report.files.len(),
        ui::bytes(report.total_size),
        ui::duration(scanned_in)
    );
    if !report.skipped.is_empty() {
        println!("  Skipped:         {}", report.skipped.len());
        for (reason, count) in report.skipped_by_reason() {
            println!("    • {count} × {reason}");
        }
    }
    println!("  Destination:     {destination}");
    println!(
        "  Estimated time:  {}",
        ui::estimate(report.total_size, rate)
    );
    println!("{}", ui::RULE);
}

// TODO(console)
fn print_nothing_to_back_up() {
    println!("Nothing to back up.");
}

// TODO(console)
fn print_dry_run(report: &ScanReport, list: bool) {
    if list {
        for file in &report.files {
            println!("  {}  ({})", file.key, ui::bytes(file.size));
        }
        if !report.skipped.is_empty() {
            println!("Skipped:");
            for issue in &report.skipped {
                println!("  {}  ({})", issue.path, issue.reason);
            }
        }
    }
    println!("✅ Dry run OK: nothing was transferred.");
}

// TODO(console)
fn print_connected(target: &SmbTarget, encrypt: bool) {
    println!(
        "🔌 Connected to {target} (encryption: {})",
        if encrypt { "on" } else { "off" }
    );
}

// TODO(console)
fn print_retention_kept() {
    println!("ℹ️  Old backups were kept because this backup has errors.");
}

// TODO(console)
fn print_removed(name: &str, keep: usize) {
    println!("🗑  Removed old backup {name} (retention: keep {keep})");
}

// TODO(console)
fn print_summary(m: &BackupManifest, target: &SmbTarget) {
    let title = match m.status {
        BackupStatus::Completed => "Backup completed successfully",
        BackupStatus::CompletedWithErrors => "Backup completed with errors",
        _ => "Backup FAILED",
    };
    println!("{}", ui::RULE);
    println!("{} {title}", m.status.icon());
    println!("{}", ui::RULE);
    println!("  Backup:      {}", m.name);
    println!(
        "  Location:    {}",
        remote::remote_display(target, &m.backup_path)
    );
    println!(
        "  Saved:       {} files ({})",
        m.file_count,
        ui::bytes(m.total_size)
    );
    if let Some(d) = m.duration() {
        let rate = m
            .throughput()
            .map(|r| format!(" — {}", ui::rate(r)))
            .unwrap_or_default();
        println!("  Duration:    {}{rate}", ui::duration(d));
    }
    if m.verified && m.file_count > 0 {
        println!("  Verified:    all saved files are present on the NAS with the right size");
    }
    if m.skipped_count > 0 {
        println!(
            "  Skipped:     {} (see `heelonbackup status --detailed`)",
            m.skipped_count
        );
    }
    let (not_done, failed): (Vec<_>, Vec<_>) = m
        .failed
        .iter()
        .partition(|i| i.reason.starts_with(NOT_TRANSFERRED));
    if !not_done.is_empty() {
        println!("  Not sent:    {} files (transfer stopped)", not_done.len());
    }
    if !failed.is_empty() {
        println!("  Failed:      {}", failed.len());
        ui::print_limited(&failed, 10, |i| format!("{}: {}", i.path, i.reason));
    }
    if let Some(notes) = &m.notes {
        println!("  Note:        {notes}");
    }
    println!("{}", ui::RULE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backup(name: &str, status: BackupStatus, partial: bool) -> (String, Option<BackupManifest>) {
        let mut m = BackupManifest::new(name.into(), String::new(), String::new(), vec![]);
        m.status = status;
        m.partial = partial;
        (name.to_string(), Some(m))
    }

    #[test]
    fn request_overrides_win_over_config() {
        let mut config = Config::default();
        config.backup.workers = 4;
        config.backup.max_file_size = 1024;
        let request = BackupRequest {
            workers: Some(2),
            max_file_size: Some(2048),
            ..Default::default()
        };
        let effective = effective_config(&config, &request);
        assert_eq!(effective.backup.workers, 2);
        assert_eq!(effective.backup.max_file_size, 2048);
    }

    #[test]
    fn config_values_used_when_no_override() {
        let mut config = Config::default();
        config.backup.workers = 3;
        config.backup.max_file_size = 999;
        let effective = effective_config(&config, &BackupRequest::default());
        assert_eq!(effective.backup.workers, 3);
        assert_eq!(effective.backup.max_file_size, 999);
    }

    #[test]
    fn retention_keeps_complete_backups() {
        use BackupStatus::*;
        let backups = vec![
            backup("20260107_000000", Completed, false), // current
            backup("20260106_000000", Failed, false),
            backup("20260105_000000", Completed, true),
            backup("20260104_000000", CompletedWithErrors, false),
            ("20260103_000000".to_string(), None),
            backup("20260102_000000", Completed, false),
            backup("20260101_000000", Completed, true),
            backup("20251231_000000", Completed, false),
        ];
        let deleted = retention_candidates(&backups, "20260107_000000", 3);
        assert_eq!(
            deleted,
            vec![
                "20260106_000000",
                "20260103_000000",
                "20260101_000000",
                "20251231_000000"
            ]
        );
        assert!(retention_candidates(&backups, "20260107_000000", 100).len() == 2);
    }
}
