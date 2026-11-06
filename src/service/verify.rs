//! Verify orchestration: check that a backup on the NAS is complete, optionally intact
//!
//! Called identically by `heelonbackup verify` (cli) and the console verify flow.

use super::{Exit, remote};
use crate::config::Config;
use crate::error::{BackupError, Result};
use crate::storage::BackupManifest;
use crate::storage::transfer::{Item, Mode, Outcome, Transfer};
use crate::ui;
use std::collections::HashMap;

pub async fn run(config: &Config, name: Option<&str>, deep: bool) -> Result<Exit> {
    let client = remote::connect(config).await?;
    let manifest = remote::resolve_backup(&client, config, name).await?;
    print_verify_header(&manifest, deep);

    let spinner = ui::spinner("Listing files on the NAS...");
    let listing = client.list(&manifest.data_dir(), true).await;
    spinner.finish_and_clear();
    let remote_files: HashMap<String, u64> = listing?
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| (e.path, e.size))
        .collect();

    let mut problems: Vec<(String, String)> = Vec::new();
    let mut present = Vec::new();
    for (key, info) in &manifest.files {
        match remote_files.get(&manifest.remote_path(key)) {
            None => problems.push((key.clone(), "missing on the NAS".into())),
            Some(&size) if size != info.size => problems.push((
                key.clone(),
                format!("{size} bytes on the NAS, {} expected", info.size),
            )),
            Some(_) => present.push((key, info)),
        }
    }

    let mut interrupted = false;
    if deep && !present.is_empty() {
        let tmp =
            tempfile::tempdir().map_err(|e| BackupError::io("cannot create temporary dir", e))?;
        let tmp_str = tmp
            .path()
            .to_str()
            .ok_or_else(|| BackupError::InvalidPath(tmp.path().display().to_string()))?
            .to_string();
        let items: Vec<Item> = present
            .iter()
            .enumerate()
            .map(|(i, (key, info))| Item {
                local: format!("{tmp_str}/{i}"),
                remote: manifest.remote_path(key),
                size: info.size,
            })
            .collect();
        let total = items.iter().map(|i| i.size).sum();
        let pb = ui::transfer_bar(total, "Checking");
        let transfer = Transfer::new(items, pb.clone());
        interrupted = tokio::select! {
            () = transfer.run(&client, Mode::Download { discard: true }, config.backup.workers) => false,
            () = super::interrupted() => true,
        };
        pb.finish_and_clear();
        for ((key, info), outcome) in present.iter().zip(transfer.outcomes()) {
            match outcome {
                Some(Outcome::Done { sha256, .. }) if sha256 == info.sha256 => {}
                Some(Outcome::Done { .. }) => {
                    problems.push(((*key).clone(), "checksum mismatch (corrupted)".into()))
                }
                Some(Outcome::Failed { reason, .. }) => problems.push(((*key).clone(), reason)),
                None if interrupted => {}
                None => problems.push(((*key).clone(), "not checked".into())),
            }
        }
    }

    print_verify_report(&manifest, &problems, interrupted, deep);

    Ok(if interrupted {
        Exit::Interrupted
    } else if !problems.is_empty() {
        Exit::Failure
    } else if !manifest.status.is_usable() || manifest.failed_count > 0 {
        Exit::Warnings
    } else {
        Exit::Success
    })
}

// ---- Display ----
// Every line goes through `ui::say!`: printed as-is by the CLI, shown in the
// terminal pane by the console.

fn print_verify_header(manifest: &BackupManifest, deep: bool) {
    ui::say!(
        "🔍 Verifying backup {} ({}, {} files, {}){}",
        manifest.name,
        ui::date(manifest.started_at),
        manifest.file_count,
        ui::bytes(manifest.total_size),
        if deep {
            " — deep mode: every file is downloaded and its checksum checked"
        } else {
            ""
        }
    );
}

fn print_verify_report(
    manifest: &BackupManifest,
    problems: &[(String, String)],
    interrupted: bool,
    deep: bool,
) {
    ui::say!("{}", ui::RULE);
    let ok = manifest.files.len() - problems.len();
    if interrupted {
        ui::say!("⏹  Verification interrupted");
    } else if problems.is_empty() {
        ui::say!(
            "✅ Backup {} is intact: {ok} files {}",
            manifest.name,
            if deep {
                "with valid checksums"
            } else {
                "present with the right size"
            }
        );
    } else {
        ui::say!(
            "❌ Backup {} has {} problem(s)",
            manifest.name,
            problems.len()
        );
        ui::print_limited(problems, 20, |(k, r)| format!("{k}: {r}"));
    }
    if manifest.failed_count > 0 {
        ui::say!(
            "ℹ️  {} file(s) had already failed during the backup (status: {})",
            manifest.failed_count,
            manifest.status.label()
        );
    }
    ui::say!("{}", ui::RULE);
}
