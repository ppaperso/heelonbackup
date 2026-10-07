//! `heelonbackup restore`

use super::common::{self, Exit};
use crate::config::{Config, is_safe_for_smbclient};
use crate::error::{BackupError, Result};
use crate::storage::transfer::{Item, Mode, Outcome, Transfer};
use crate::storage::{BackupManifest, FileInfo, LocalHistory};
use crate::ui;
use clap::Args;
use nix::unistd::{AccessFlags, access};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Args, Debug)]
pub struct RestoreArgs {
    /// Backup name (YYYYMMDD_HHMMSS) [default: latest]
    pub backup: Option<String>,

    /// Restore under this folder instead of the original locations
    #[arg(short, long, value_name = "DIR")]
    pub target: Option<PathBuf>,

    /// Only restore this file or folder (original absolute path; repeatable)
    #[arg(short, long = "path", value_name = "PATH")]
    pub paths: Vec<String>,

    /// Check everything (NAS content, conflicts, free space, permissions) without writing anything
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Replace existing files that differ from the backup
    #[arg(long)]
    pub overwrite: bool,

    /// Print the action planned for every file
    #[arg(short, long)]
    pub list: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Create,
    Overwrite,
    /// Same size and modification time: nothing to do
    Identical,
    /// Exists and differs, kept because --overwrite was not given
    Conflict,
    /// Cannot be restored (reason in `Planned::problem`)
    Blocked,
}

struct Planned<'a> {
    key: &'a str,
    info: &'a FileInfo,
    dest: PathBuf,
    action: Action,
    problem: Option<String>,
}

/// Manifest keys must be clean absolute paths: never restore outside of the destination
fn is_valid_key(key: &str) -> bool {
    key.strip_prefix('/').is_some_and(|rest| {
        rest.split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
    }) && is_safe_for_smbclient(key)
}

fn matches_prefix(key: &str, prefixes: &[String]) -> bool {
    prefixes.is_empty()
        || prefixes.iter().any(|p| {
            let p = p.trim_end_matches('/');
            p.is_empty() || key == p || key.strip_prefix(p).is_some_and(|r| r.starts_with('/'))
        })
}

fn destination(key: &str, target: Option<&Path>) -> PathBuf {
    match target {
        Some(t) => t.join(key.trim_start_matches('/')),
        None => PathBuf::from(key),
    }
}

fn classify(dest: &Path, info: &FileInfo, overwrite: bool) -> (Action, Option<String>) {
    match fs::symlink_metadata(dest) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Action::Create, None),
        Err(e) => (
            Action::Blocked,
            Some(format!("cannot check destination: {e}")),
        ),
        Ok(meta) if meta.is_dir() => (
            Action::Blocked,
            Some("a directory exists at the destination".into()),
        ),
        Ok(meta) => {
            let same = meta.is_file()
                && meta.len() == info.size
                && info.mtime.is_none_or(|t| t == meta.mtime());
            if same {
                (Action::Identical, None)
            } else if overwrite {
                (Action::Overwrite, None)
            } else {
                (Action::Conflict, None)
            }
        }
    }
}

fn first_existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors()
        .skip(1)
        .find(|p| p.symlink_metadata().is_ok())
}

/// Free space and write permission checks for the files to restore
fn check_destinations(plan: &mut [Planned<'_>]) -> Vec<String> {
    let mut problems = Vec::new();
    // device -> (mount point example, bytes needed, bytes available)
    let mut space: BTreeMap<u64, (PathBuf, u64, u64)> = BTreeMap::new();
    let mut checked: HashMap<PathBuf, Option<String>> = HashMap::new();
    for p in plan
        .iter_mut()
        .filter(|p| matches!(p.action, Action::Create | Action::Overwrite))
    {
        let Some(ancestor) = first_existing_ancestor(&p.dest) else {
            p.action = Action::Blocked;
            p.problem = Some("no existing parent folder".into());
            continue;
        };
        let problem = checked
            .entry(ancestor.to_path_buf())
            .or_insert_with(|| {
                let meta = match fs::metadata(ancestor) {
                    Ok(meta) => meta,
                    Err(e) => return Some(format!("cannot access {}: {e}", ancestor.display())),
                };
                if !meta.is_dir() {
                    return Some(format!("{} is not a folder", ancestor.display()));
                }
                if access(ancestor, AccessFlags::W_OK | AccessFlags::X_OK).is_err() {
                    return Some(format!("{} is not writable", ancestor.display()));
                }
                let available = nix::sys::statvfs::statvfs(ancestor)
                    .map(|s| s.blocks_available() * s.fragment_size())
                    .unwrap_or(u64::MAX);
                space
                    .entry(meta.dev())
                    .or_insert((ancestor.to_path_buf(), 0, available));
                None
            })
            .clone();
        if let Some(problem) = problem {
            p.action = Action::Blocked;
            p.problem = Some(problem);
            continue;
        }
        if let Ok(meta) = fs::metadata(ancestor)
            && let Some(entry) = space.get_mut(&meta.dev())
        {
            entry.1 += p.info.size;
        }
    }
    for (path, needed, available) in space.values() {
        // keep a small margin for the temporary files and the file system
        if needed.saturating_add(needed / 100) > *available {
            problems.push(format!(
                "not enough free space on the file system of {}: {} needed, {} available",
                path.display(),
                ui::bytes(*needed),
                ui::bytes(*available)
            ));
        } else {
            println!(
                "  Free space:      OK on {} ({} needed, {} available)",
                path.display(),
                ui::bytes(*needed),
                ui::bytes(*available)
            );
        }
    }
    problems
}

pub async fn run(config: &Config, args: RestoreArgs) -> Result<Exit> {
    let target = match &args.target {
        Some(t) => {
            Some(std::path::absolute(t).map_err(|e| BackupError::io("invalid --target", e))?)
        }
        None => None,
    };
    let client = common::connect(config).await?;
    let manifest = common::resolve_backup(&client, config, args.backup.as_deref()).await?;

    let selected: Vec<(&String, &FileInfo)> = manifest
        .files
        .iter()
        .filter(|(k, _)| matches_prefix(k, &args.paths))
        .collect();
    if selected.is_empty() {
        return Err(BackupError::BackupNotFound(format!(
            "no file of backup {} matches {}",
            manifest.name,
            args.paths.join(", ")
        )));
    }

    let spinner = ui::spinner("Checking the backup on the NAS...");
    let listing = client.list(&manifest.data_dir(), true).await;
    spinner.finish_and_clear();
    let remote: HashMap<String, u64> = listing?
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| (e.path, e.size))
        .collect();

    let mut plan: Vec<Planned<'_>> = selected
        .into_iter()
        .map(|(key, info)| {
            let dest = destination(key, target.as_deref());
            let (action, problem) = if !is_valid_key(key) {
                (Action::Blocked, Some("unsafe path in manifest".to_string()))
            } else {
                match remote.get(&manifest.remote_path(key)) {
                    None => (Action::Blocked, Some("missing on the NAS".to_string())),
                    Some(&size) if size != info.size => (
                        Action::Blocked,
                        Some(format!("{size} bytes on the NAS, {} expected", info.size)),
                    ),
                    Some(_) => classify(&dest, info, args.overwrite),
                }
            };
            Planned {
                key,
                info,
                dest,
                action,
                problem,
            }
        })
        .collect();

    println!(
        "🔁 Restore plan{}",
        if args.dry_run {
            " (dry run: nothing will be written)"
        } else {
            ""
        }
    );
    println!("{}", ui::RULE);
    println!(
        "  Backup:          {} ({}, {} from {}, {})",
        manifest.name,
        common::date(manifest.started_at),
        common::age(manifest.started_at),
        manifest.hostname,
        manifest.status.label()
    );
    println!(
        "  Destination:     {}",
        target
            .as_ref()
            .map_or("original locations".to_string(), |t| t
                .display()
                .to_string())
    );
    let space_problems = check_destinations(&mut plan);
    print_plan(&plan, &manifest, args.list);
    for problem in &space_problems {
        println!("  ❌ {problem}");
    }
    println!("{}", ui::RULE);

    let to_restore: Vec<&Planned<'_>> = plan
        .iter()
        .filter(|p| matches!(p.action, Action::Create | Action::Overwrite))
        .collect();
    let blocked = plan.iter().filter(|p| p.action == Action::Blocked).count();
    let conflicts = plan.iter().filter(|p| p.action == Action::Conflict).count();
    let identical = plan
        .iter()
        .filter(|p| p.action == Action::Identical)
        .count();
    let nothing_possible = to_restore.is_empty() && identical == 0 && blocked > 0;

    if args.dry_run {
        let (icon, text, exit) = if !space_problems.is_empty() || nothing_possible {
            ("❌", "the restore would fail", Exit::Failure)
        } else if blocked > 0 || conflicts > 0 {
            (
                "⚠️",
                "the restore would be partial (see above)",
                Exit::Warnings,
            )
        } else {
            ("✅", "the restore would succeed", Exit::Success)
        };
        println!("{icon} Dry run: {text}.");
        return Ok(exit);
    }
    if !space_problems.is_empty() {
        return Err(BackupError::InvalidPath(space_problems.join("; ")));
    }
    if to_restore.is_empty() {
        println!("Nothing to restore.");
        return Ok(match (nothing_possible, blocked + conflicts) {
            (true, _) => Exit::Failure,
            (false, 0) => Exit::Success,
            (false, _) => Exit::Warnings,
        });
    }

    execute(config, &client, &manifest, &to_restore, blocked, conflicts).await
}

fn print_plan(plan: &[Planned<'_>], manifest: &BackupManifest, list: bool) {
    let count = |a: Action| plan.iter().filter(|p| p.action == a).collect::<Vec<_>>();
    let create = count(Action::Create);
    let overwrite = count(Action::Overwrite);
    let restore_bytes: u64 = create.iter().chain(&overwrite).map(|p| p.info.size).sum();
    println!(
        "  To restore:      {} files ({}){}",
        create.len() + overwrite.len(),
        ui::bytes(restore_bytes),
        if overwrite.is_empty() {
            String::new()
        } else {
            format!(", {} will replace an existing file", overwrite.len())
        }
    );
    let identical = count(Action::Identical);
    if !identical.is_empty() {
        println!(
            "  Already present: {} identical files (skipped)",
            identical.len()
        );
    }
    let conflicts = count(Action::Conflict);
    if !conflicts.is_empty() {
        println!(
            "  ⚠️  Conflicts:    {} existing files differ and will be kept (use --overwrite to replace them)",
            conflicts.len()
        );
        if !list {
            common::print_limited(&conflicts, 10, |p| p.dest.display().to_string());
        }
    }
    let blocked = count(Action::Blocked);
    if !blocked.is_empty() {
        println!("  ❌ Cannot restore: {} files", blocked.len());
        if !list {
            common::print_limited(&blocked, 10, |p| {
                format!("{}: {}", p.key, p.problem.as_deref().unwrap_or("?"))
            });
        }
    }
    if manifest.failed_count > 0 {
        println!(
            "  ℹ️  {} files had failed during this backup and are not in it",
            manifest.failed_count
        );
    }
    println!(
        "  Estimated time:  {}",
        ui::estimate(restore_bytes, LocalHistory::open().measured_rate())
    );
    if list {
        for p in plan {
            let label = match p.action {
                Action::Create => "restore  ",
                Action::Overwrite => "overwrite",
                Action::Identical => "identical",
                Action::Conflict => "conflict ",
                Action::Blocked => "BLOCKED  ",
            };
            let problem = p
                .problem
                .as_deref()
                .map(|r| format!("  ({r})"))
                .unwrap_or_default();
            println!("    {label} {}{problem}", p.dest.display());
        }
    }
}

async fn execute(
    config: &Config,
    client: &crate::storage::SmbClient,
    manifest: &BackupManifest,
    to_restore: &[&Planned<'_>],
    blocked: usize,
    conflicts: usize,
) -> Result<Exit> {
    let started = std::time::Instant::now();
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut items = Vec::new();
    let mut planned = Vec::new();
    for (idx, p) in to_restore.iter().enumerate() {
        let parent = p.dest.parent().unwrap_or(Path::new("/"));
        if let Err(e) = fs::create_dir_all(parent) {
            failures.push((
                p.dest.display().to_string(),
                format!("cannot create folder: {e}"),
            ));
            continue;
        }
        let part = parent.join(format!(".heelonbackup-{idx}.part"));
        let Some(local) = part.to_str().filter(|s| is_safe_for_smbclient(s)) else {
            failures.push((
                p.dest.display().to_string(),
                "unsupported destination path".into(),
            ));
            continue;
        };
        items.push(Item {
            local: local.to_string(),
            remote: manifest.remote_path(p.key),
            size: p.info.size,
        });
        planned.push((*p, part));
    }

    let total: u64 = items.iter().map(|i| i.size).sum();
    let pb = ui::transfer_bar(total, "Restoring");
    let transfer = Transfer::new(items, pb.clone());
    let interrupted = tokio::select! {
        () = transfer.run(client, Mode::Download { discard: false }, config.backup.workers) => false,
        _ = tokio::signal::ctrl_c() => true,
    };
    pb.finish_and_clear();

    let mut restored = 0u64;
    let mut restored_bytes = 0u64;
    for ((p, part), outcome) in planned.iter().zip(transfer.outcomes()) {
        let result = match outcome {
            Some(Outcome::Done { sha256, .. }) if sha256 == p.info.sha256 => finalize(part, p),
            Some(Outcome::Done { .. }) => {
                Err("checksum mismatch: the copy on the NAS is corrupted".into())
            }
            Some(Outcome::Failed { reason, .. }) => Err(reason),
            None => Err(if interrupted {
                "not restored: interrupted".to_string()
            } else {
                format!(
                    "not restored: {}",
                    transfer.fatal_error().unwrap_or_default()
                )
            }),
        };
        match result {
            Ok(()) => {
                restored += 1;
                restored_bytes += p.info.size;
            }
            Err(reason) => {
                let _ = fs::remove_file(part);
                failures.push((p.dest.display().to_string(), reason));
            }
        }
    }

    let (icon, title) = if interrupted {
        ("⏹", "Restore interrupted")
    } else if failures.is_empty() && blocked == 0 {
        ("✅", "Restore completed successfully")
    } else if restored > 0 {
        ("⚠️", "Restore completed with errors")
    } else {
        ("❌", "Restore FAILED")
    };
    println!("{}", ui::RULE);
    println!("{icon} {title}");
    println!("{}", ui::RULE);
    println!(
        "  Restored:        {restored} files ({}), checksums verified",
        ui::bytes(restored_bytes)
    );
    println!("  Duration:        {}", ui::duration(started.elapsed()));
    if conflicts > 0 {
        println!("  Kept (conflict): {conflicts}");
    }
    if blocked > 0 {
        println!("  Not restorable:  {blocked}");
    }
    if !failures.is_empty() {
        println!("  Failed:          {}", failures.len());
        common::print_limited(&failures, 10, |(p, r)| format!("{p}: {r}"));
    }
    println!("{}", ui::RULE);

    Ok(if interrupted {
        Exit::Interrupted
    } else if failures.is_empty() && blocked == 0 {
        Exit::Success
    } else if restored > 0 {
        Exit::Warnings
    } else {
        Exit::Failure
    })
}

/// Restore permissions and modification time, then atomically move the file in place
fn finalize(part: &Path, p: &Planned<'_>) -> std::result::Result<(), String> {
    let err = |what: &str, e: std::io::Error| format!("{what}: {e}");
    if let Some(mode) = p.info.mode {
        // never restore setuid/setgid bits
        fs::set_permissions(part, fs::Permissions::from_mode(mode & 0o1777))
            .map_err(|e| err("cannot set permissions", e))?;
    }
    if let Some(mtime) = p.info.mtime.and_then(|t| u64::try_from(t).ok()) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(part)
            .map_err(|e| err("cannot open restored file", e))?;
        file.set_modified(UNIX_EPOCH + Duration::from_secs(mtime))
            .map_err(|e| err("cannot set modification time", e))?;
    }
    fs::rename(part, &p.dest).map_err(|e| err("cannot move file into place", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_keys() {
        assert!(is_valid_key("/home/u/a b.txt"));
        assert!(!is_valid_key("home/u/a"));
        assert!(!is_valid_key("/home/../etc/passwd"));
        assert!(!is_valid_key("/home/./u"));
        assert!(!is_valid_key("/"));
        assert!(!is_valid_key("/a\"b"));
    }

    #[test]
    fn prefixes() {
        let p = vec!["/home/u/Docs/".to_string()];
        assert!(matches_prefix("/home/u/Docs/a.txt", &p));
        assert!(matches_prefix("/home/u/Docs", &p));
        assert!(!matches_prefix("/home/u/Docs2/a.txt", &p));
        assert!(matches_prefix("/x", &[]));
    }

    #[test]
    fn destinations() {
        assert_eq!(
            destination("/home/u/a", Some(Path::new("/tmp/r"))),
            PathBuf::from("/tmp/r/home/u/a")
        );
        assert_eq!(destination("/home/u/a", None), PathBuf::from("/home/u/a"));
    }
}
