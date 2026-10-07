//! `heelonbackup status` (local history) and `heelonbackup list` (backups on the NAS)

use super::common::{self, Exit};
use crate::config::Config;
use crate::error::Result;
use crate::storage::{BackupManifest, BackupStatus, LocalHistory};
use crate::ui;
use chrono::Local;

const STALE_DAYS: i64 = 7;

pub fn status(all: bool, detailed: bool) -> Exit {
    let history = LocalHistory::open().load_all();
    let Some(last) = history.first() else {
        println!("No backup recorded on this computer yet. Run `heelonbackup backup`.");
        return Exit::Warnings;
    };

    println!("📊 Last backup");
    println!("{}", ui::RULE);
    print_details(last, detailed);
    println!("{}", ui::RULE);

    let last_ok = history.iter().find(|m| m.status.is_usable());
    let exit = match last_ok {
        None => {
            println!("❌ No successful backup recorded.");
            Exit::Failure
        }
        Some(ok) if (Local::now() - ok.started_at).num_days() >= STALE_DAYS => {
            println!(
                "⚠️  The last successful backup is {} old ({}). Time for a new one!",
                common::age(ok.started_at).trim_end_matches(" ago"),
                ok.name
            );
            Exit::Warnings
        }
        Some(ok) if ok.name != last.name => {
            println!(
                "ℹ️  Last successful backup: {} ({})",
                ok.name,
                common::age(ok.started_at)
            );
            Exit::Warnings
        }
        Some(ok) if ok.status == BackupStatus::CompletedWithErrors => Exit::Warnings,
        Some(_) => Exit::Success,
    };

    if all && history.len() > 1 {
        println!();
        println!("📚 History ({} backups)", history.len());
        print_table(&history);
    }
    exit
}

fn print_details(m: &BackupManifest, detailed: bool) {
    println!(
        "  Status:      {} {}{}",
        m.status.icon(),
        m.status.label(),
        if m.partial {
            " (partial: selected paths only)"
        } else {
            ""
        }
    );
    println!("  Backup:      {}", m.name);
    println!(
        "  Date:        {} ({})",
        common::date(m.started_at),
        common::age(m.started_at)
    );
    println!("  Location:    {}/{}", m.server, m.backup_path);
    println!(
        "  Sources:     {}{}",
        m.sources.join(", "),
        if m.partial { " (specific paths)" } else { "" }
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
    if m.failed_count > 0 {
        println!("  Failed:      {}", m.failed_count);
    }
    if m.skipped_count > 0 {
        println!("  Skipped:     {}", m.skipped_count);
    }
    if let Some(notes) = &m.notes {
        println!("  Note:        {notes}");
    }
    if detailed {
        if !m.failed.is_empty() {
            println!("  Failed files:");
            common::print_limited(&m.failed, usize::MAX, |i| {
                format!("{}: {}", i.path, i.reason)
            });
        }
        if !m.skipped.is_empty() {
            println!("  Skipped files:");
            common::print_limited(&m.skipped, usize::MAX, |i| {
                format!("{}: {}", i.path, i.reason)
            });
        }
        let shown = m.failed.len().max(m.skipped.len()) as u64;
        if shown < m.failed_count.max(m.skipped_count) {
            println!(
                "  (full lists are in {}/manifest.json on the NAS)",
                m.backup_path
            );
        }
    }
}

fn print_table(list: &[BackupManifest]) {
    println!(
        "  {:<17} {:<24} {:>9} {:>10} {:>11}  HOST",
        "NAME", "STATUS", "FILES", "SIZE", "DURATION"
    );
    for m in list {
        println!(
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

pub async fn list(config: &Config) -> Result<Exit> {
    let client = common::connect(config).await?;
    let base = config.remote_base()?;
    let spinner = ui::spinner("Reading backups on the NAS...");
    let result = common::load_summaries(&client, &base).await;
    spinner.finish_and_clear();
    let backups = result?;

    let target = crate::config::SmbTarget::parse(&config.smb.url)?;
    if backups.is_empty() {
        println!(
            "No backup found in {}.",
            common::remote_display(&target, &base)
        );
        return Ok(Exit::Success);
    }
    println!(
        "🗄  {} backup(s) in {} (retention: {})",
        backups.len(),
        common::remote_display(&target, &base),
        match config.storage.retention {
            0 => "keep all".to_string(),
            n => format!("keep {n}"),
        }
    );
    let (complete, incomplete): (Vec<_>, Vec<_>) =
        backups.into_iter().partition(|(_, s)| s.is_some());
    let summaries: Vec<BackupManifest> = complete.into_iter().filter_map(|(_, s)| s).collect();
    print_table(&summaries);
    for (name, _) in incomplete {
        println!("  {name:<17} ❓ no summary (interrupted or not created by this tool)");
    }
    Ok(Exit::Success)
}
