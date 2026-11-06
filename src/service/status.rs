//! Status orchestration: local history (`status`) and backups on the NAS (`list`)
//!
//! Called identically by `heelonbackup status` / `heelonbackup list` (cli)
//! and the console status flow.

use super::{Exit, remote};
use crate::config::{Config, SmbTarget};
use crate::error::Result;
use crate::storage::smb_client::Reachability;
use crate::storage::{BackupManifest, BackupStatus, LocalHistory};
use crate::ui;
use chrono::Local;

/// A last successful backup older than this is flagged as stale
pub(crate) const STALE_DAYS: i64 = 7;

pub fn status(all: bool, detailed: bool) -> Exit {
    let history = LocalHistory::open().load_all();
    let Some(last) = history.first() else {
        print_no_history();
        return Exit::Warnings;
    };

    print_last_backup(last, detailed);
    let exit = print_verdict(&history, last);

    if all && history.len() > 1 {
        print_history(&history);
    }
    exit
}

/// Is the NAS reachable? Online = success, offline = failure (no password needed)
pub async fn ping(config: &Config) -> Result<Exit> {
    let (target, state) = remote::reachability(config).await?;
    let address = remote::probed_address(&target);
    Ok(match state {
        Reachability::Online(latency) => {
            ui::say!("🟢 NAS online: {address} ({} ms)", latency.as_millis());
            Exit::Success
        }
        Reachability::Offline(reason) => {
            ui::say!("🔴 NAS offline: {address} ({reason})");
            Exit::Failure
        }
    })
}

pub async fn list(config: &Config) -> Result<Exit> {
    let client = remote::connect(config).await?;
    let base = config.remote_base()?;
    let spinner = ui::spinner("Reading backups on the NAS...");
    let result = remote::load_summaries(&client, &base).await;
    spinner.finish_and_clear();
    let backups = result?;

    let target = SmbTarget::parse(&config.smb.url)?;
    let location = remote::remote_display(&target, &base);
    if backups.is_empty() {
        print_no_backups(&location);
        return Ok(Exit::Success);
    }
    print_list_header(backups.len(), &location, config.storage.retention);
    let (complete, incomplete): (Vec<_>, Vec<_>) =
        backups.into_iter().partition(|(_, s)| s.is_some());
    let summaries: Vec<BackupManifest> = complete.into_iter().filter_map(|(_, s)| s).collect();
    print_table(&summaries);
    print_incomplete(&incomplete);
    Ok(Exit::Success)
}

// ---- Display ----
// Every line goes through `ui::say!`: printed as-is by the CLI, shown in the
// terminal pane by the console.

fn print_no_history() {
    ui::say!("No backup recorded on this computer yet. Run `heelonbackup backup`.");
}

fn print_last_backup(m: &BackupManifest, detailed: bool) {
    ui::say!("📊 Last backup");
    ui::say!("{}", ui::RULE);
    print_details(m, detailed);
    ui::say!("{}", ui::RULE);
}

/// Verdict on the recorded backups (none successful, stale, ...); also returns the exit code
fn print_verdict(history: &[BackupManifest], last: &BackupManifest) -> Exit {
    let last_ok = history.iter().find(|m| m.status.is_usable());
    match last_ok {
        None => {
            ui::say!("❌ No successful backup recorded.");
            Exit::Failure
        }
        Some(ok) if (Local::now() - ok.started_at).num_days() >= STALE_DAYS => {
            ui::say!(
                "⚠️  The last successful backup is {} old ({}). Time for a new one!",
                ui::age(ok.started_at).trim_end_matches(" ago"),
                ok.name
            );
            Exit::Warnings
        }
        Some(ok) if ok.name != last.name => {
            ui::say!(
                "ℹ️  Last successful backup: {} ({})",
                ok.name,
                ui::age(ok.started_at)
            );
            Exit::Warnings
        }
        Some(ok) if ok.status == BackupStatus::CompletedWithErrors => Exit::Warnings,
        Some(_) => Exit::Success,
    }
}

fn print_history(history: &[BackupManifest]) {
    ui::say!();
    ui::say!("📚 History ({} backups)", history.len());
    print_table(history);
}

fn print_details(m: &BackupManifest, detailed: bool) {
    ui::say!(
        "  Status:      {} {}{}",
        m.status.icon(),
        m.status.label(),
        if m.partial {
            " (partial: selected paths only)"
        } else {
            ""
        }
    );
    ui::say!("  Backup:      {}", m.name);
    ui::say!(
        "  Date:        {} ({})",
        ui::date(m.started_at),
        ui::age(m.started_at)
    );
    ui::say!("  Location:    {}/{}", m.server, m.backup_path);
    ui::say!(
        "  Sources:     {}{}",
        m.sources.join(", "),
        if m.partial { " (specific paths)" } else { "" }
    );
    ui::say!(
        "  Saved:       {} files ({})",
        m.file_count,
        ui::bytes(m.total_size)
    );
    if let Some(d) = m.duration() {
        let rate = m
            .throughput()
            .map(|r| format!(" — {}", ui::rate(r)))
            .unwrap_or_default();
        ui::say!("  Duration:    {}{rate}", ui::duration(d));
    }
    if m.failed_count > 0 {
        ui::say!("  Failed:      {}", m.failed_count);
    }
    if m.skipped_count > 0 {
        ui::say!("  Skipped:     {}", m.skipped_count);
    }
    if let Some(notes) = &m.notes {
        ui::say!("  Note:        {notes}");
    }
    if detailed {
        if !m.failed.is_empty() {
            ui::say!("  Failed files:");
            ui::print_limited(&m.failed, usize::MAX, |i| {
                format!("{}: {}", i.path, i.reason)
            });
        }
        if !m.skipped.is_empty() {
            ui::say!("  Skipped files:");
            ui::print_limited(&m.skipped, usize::MAX, |i| {
                format!("{}: {}", i.path, i.reason)
            });
        }
        let shown = m.failed.len().max(m.skipped.len()) as u64;
        if shown < m.failed_count.max(m.skipped_count) {
            ui::say!(
                "  (full lists are in {}/manifest.json on the NAS)",
                m.backup_path
            );
        }
    }
}

fn print_table(list: &[BackupManifest]) {
    ui::say!(
        "  {:<17} {:<24} {:>9} {:>10} {:>11}  HOST",
        "NAME",
        "STATUS",
        "FILES",
        "SIZE",
        "DURATION"
    );
    for m in list {
        ui::say!(
            "  {:<17} {:<24} {:>9} {:>10} {:>11}  {}",
            m.name,
            format!(
                "{} {}{}",
                m.status.icon(),
                m.status.label(),
                if m.partial { " (partial)" } else { "" }
            ),
            m.file_count,
            ui::bytes(m.total_size),
            m.duration().map(ui::duration).unwrap_or_else(|| "-".into()),
            m.hostname
        );
    }
}

fn print_no_backups(location: &str) {
    ui::say!("No backup found in {location}.");
}

fn print_list_header(count: usize, location: &str, retention: usize) {
    ui::say!(
        "🗄  {count} backup(s) in {location} (retention: {})",
        match retention {
            0 => "keep all".to_string(),
            n => format!("keep {n}"),
        }
    );
}

fn print_incomplete(incomplete: &[(String, Option<BackupManifest>)]) {
    for (name, _) in incomplete {
        ui::say!("  {name:<17} ❓ no summary (interrupted or not created by this tool)");
    }
}
