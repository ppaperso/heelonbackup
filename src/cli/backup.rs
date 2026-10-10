//! `heelonbackup backup`

use super::common::{self, Exit};
use crate::backup::{Excludes, ScanReport, scan};
use crate::config::Config;
use crate::error::{BackupError, Result};
use crate::storage::backup_manifest::{
    MANIFEST_FILE, NOT_TRANSFERRED, SUMMARY_FILE, new_backup_name,
};
use crate::storage::transfer::{Item, Mode, Outcome, Transfer};
use crate::storage::{BackupManifest, BackupStatus, FileInfo, LocalHistory, SmbClient};
use crate::ui;
use chrono::Local;
use clap::Args;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tracing::warn;

#[derive(Args, Debug)]
pub struct BackupArgs {
    /// Files or folders to back up instead of the configured sources
    pub paths: Vec<PathBuf>,

    /// Scan and check the connection, but transfer nothing
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// With --dry-run: print every file that would be backed up
    #[arg(short, long, requires = "dry_run")]
    pub list: bool,
}

fn resolve_sources(config: &Config, args: &BackupArgs) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    if args.paths.is_empty() {
        for source in &config.backup.sources {
            let path = common::expand_tilde(source);
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
        for path in &args.paths {
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

pub async fn run(config: &Config, args: BackupArgs) -> Result<Exit> {
    let sources = resolve_sources(config, &args)?;
    let started = Instant::now();
    let report = scan_sources(config, sources.clone()).await?;
    let history = LocalHistory::open();
    let base = config.remote_base()?;
    let target = crate::config::SmbTarget::parse(&config.smb.url)?;
    let name = new_backup_name(Local::now());

    println!(
        "📦 Backup plan{}",
        if args.dry_run { " (dry run)" } else { "" }
    );
    println!("{}", ui::RULE);
    for source in &sources {
        println!("  Source:          {}", source.display());
    }
    println!(
        "  Files:           {} ({})  [scanned in {}]",
        report.files.len(),
        ui::bytes(report.total_size),
        ui::duration(started.elapsed())
    );
    if !report.skipped.is_empty() {
        println!("  Skipped:         {}", report.skipped.len());
        for (reason, count) in report.skipped_by_reason() {
            println!("    • {count} × {reason}");
        }
    }
    println!(
        "  Destination:     {}",
        common::remote_display(&target, &format!("{base}/{name}"))
    );
    println!(
        "  Estimated time:  {}",
        ui::estimate(report.total_size, history.measured_rate())
    );
    println!("{}", ui::RULE);

    if report.files.is_empty() {
        println!("Nothing to back up.");
        return Ok(if report.skipped.is_empty() {
            Exit::Success
        } else {
            Exit::Warnings
        });
    }

    if args.dry_run {
        if args.list {
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
        return Ok(Exit::Success);
    }

    let client = common::connect(config).await?;
    println!(
        "🔌 Connected to {target} (encryption: {})",
        on_off(config.smb.encrypt)
    );

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
    manifest.partial = !args.paths.is_empty();
    let exit = execute(config, &client, &history, &mut manifest, &report).await;
    if let Err(e) = &exit {
        manifest.fail(e.to_string());
        save_history(&history, &manifest);
    }
    let exit = exit?;
    print_summary(&manifest, &target);
    Ok(exit)
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
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
        println!("ℹ️  Old backups were kept because this backup has errors.");
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
    let backups = match common::load_summaries(client, &base).await {
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
                println!("🗑  Removed old backup {name} (retention: keep {keep})");
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

fn print_summary(m: &BackupManifest, target: &crate::config::SmbTarget) {
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
        common::remote_display(target, &m.backup_path)
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
        common::print_limited(&failed, 10, |i| format!("{}: {}", i.path, i.reason));
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
