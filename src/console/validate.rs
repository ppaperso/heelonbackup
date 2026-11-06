//! Input validators of the console flows — pure functions, tested without a terminal

use crate::error::{BackupError, Result};
use crate::service::expand_tilde;
use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
