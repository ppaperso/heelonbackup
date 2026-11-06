//! Input validators of the console flows — pure functions, tested without a terminal

use crate::error::{BackupError, Result};
use crate::service::expand_tilde;
use crate::storage::backup_manifest::is_backup_name;
use std::path::PathBuf;

/// A backup name must look like YYYYMMDD_HHMMSS
pub(crate) fn validate_backup_name(name: &str) -> Result<()> {
    if is_backup_name(name) {
        Ok(())
    } else {
        Err(BackupError::BackupNotFound(format!(
            "`{name}` is not a backup name (expected YYYYMMDD_HHMMSS)"
        )))
    }
}

/// Split a comma-separated list of paths, trimming and dropping empty entries
pub(crate) fn parse_path_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Canonicalize user-provided paths (`~` allowed); every one must exist
pub(crate) fn existing_paths(paths: &[String]) -> Result<Vec<PathBuf>> {
    paths
        .iter()
        .map(|p| {
            let expanded = expand_tilde(p);
            expanded
                .canonicalize()
                .map_err(|e| BackupError::InvalidPath(format!("{}: {e}", expanded.display())))
        })
        .collect()
}

/// Index 0 of a backup picker = latest usable backup; otherwise the picked
/// name must be a valid backup name
pub(crate) fn picked_backup_name(index: usize, items: &[&str]) -> Result<Option<String>> {
    if index == 0 {
        return Ok(None);
    }
    let name = items
        .get(index)
        .ok_or_else(|| BackupError::BackupNotFound("no such backup in the list".into()))?;
    validate_backup_name(name)?;
    Ok(Some(name.to_string()))
}

/// Target folder of a restore: empty input = original locations
pub(crate) fn parse_target(text: &str) -> Option<PathBuf> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names() {
        assert!(validate_backup_name("20261010_094120").is_ok());
        assert!(validate_backup_name("latest").is_err());
        assert!(validate_backup_name("2026-10-10").is_err());
        assert!(validate_backup_name("").is_err());
    }

    #[test]
    fn path_lists() {
        assert_eq!(parse_path_list(""), Vec::<String>::new());
        assert_eq!(parse_path_list(" , ,"), Vec::<String>::new());
        assert_eq!(
            parse_path_list("/a, /b ,,/c"),
            vec!["/a".to_string(), "/b".to_string(), "/c".to_string()]
        );
    }

    #[test]
    fn existing_paths_must_exist() {
        let dir = tempfile::tempdir().unwrap();
        let ok = existing_paths(&[dir.path().display().to_string()]).unwrap();
        assert_eq!(ok, vec![dir.path().canonicalize().unwrap()]);
        assert!(existing_paths(&["/nonexistent/heelonbackup".to_string()]).is_err());
        assert!(existing_paths(&[]).unwrap().is_empty());
    }

    #[test]
    fn backup_picking() {
        let items = ["Latest usable backup", "20261010_094120", "20261010_100512"];
        assert_eq!(picked_backup_name(0, &items).unwrap(), None);
        assert_eq!(
            picked_backup_name(1, &items).unwrap(),
            Some("20261010_094120".to_string())
        );
        assert!(picked_backup_name(9, &items).is_err());
        let bad = ["Latest usable backup", "not-a-name"];
        assert!(picked_backup_name(1, &bad).is_err());
    }

    #[test]
    fn target_parsing() {
        assert_eq!(parse_target(""), None);
        assert_eq!(parse_target("   "), None);
        assert_eq!(
            parse_target(" /tmp/restore "),
            Some(PathBuf::from("/tmp/restore"))
        );
    }
}
