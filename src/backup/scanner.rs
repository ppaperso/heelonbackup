//! File scanner for discovering files to back up

use crate::config::is_safe_for_smbclient;
use crate::error::ConfigError;
use crate::storage::FileIssue;
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::fs::{self, FileType, Metadata};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tracing::debug;

pub const UNSUPPORTED_NAME: &str =
    "name is not valid UTF-8 or contains a quote, backslash or control character";

/// Header of a `CACHEDIR.TAG` file (https://bford.info/cachedir/), written by Cargo
/// in `target/` and by other tools in their cache folders
const CACHEDIR_TAG_SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";

/// True if `dir` is marked as a cache folder that can be regenerated
fn is_cache_dir(dir: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; CACHEDIR_TAG_SIGNATURE.len()];
    fs::File::open(dir.join("CACHEDIR.TAG"))
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok_and(|()| header == CACHEDIR_TAG_SIGNATURE)
}

/// True if `dir` is a Python virtual environment (PEP 405), which can be recreated
/// from the project requirements, whatever its name
fn is_python_venv(dir: &Path) -> bool {
    dir.join("pyvenv.cfg").is_file()
}

/// A regular file selected for backup
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    /// Absolute UTF-8 path, used as the key in the manifest
    pub key: String,
    pub size: u64,
    /// Modification time (seconds since the Unix epoch)
    pub mtime: Option<i64>,
    pub mode: u32,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    /// Files sorted by path, without duplicates
    pub files: Vec<ScannedFile>,
    pub skipped: Vec<FileIssue>,
    pub total_size: u64,
}

/// Compiled exclude patterns
///
/// - without `/`: matched against every file or directory name (`node_modules`, `*.tmp`)
/// - with `/`: matched against the end of the path (`.local/share/Trash`)
/// - starting with `/`: matched against the absolute path (`/home/me/Videos/**`)
#[derive(Debug, Clone)]
pub struct Excludes {
    names: GlobSet,
    paths: GlobSet,
}

impl Excludes {
    pub fn new(patterns: &[String]) -> Result<Self, ConfigError> {
        let invalid = |p: &str, e: globset::Error| {
            ConfigError::Invalid(format!("invalid exclude pattern `{p}`: {e}"))
        };
        let mut names = GlobSetBuilder::new();
        let mut paths = GlobSetBuilder::new();
        for raw in patterns {
            let pattern = raw.trim().trim_end_matches('/');
            if pattern.is_empty() {
                continue;
            }
            if pattern.contains('/') {
                let full = if pattern.starts_with('/') {
                    pattern.to_string()
                } else {
                    format!("**/{pattern}")
                };
                let glob = GlobBuilder::new(&full)
                    .literal_separator(true)
                    .build()
                    .map_err(|e| invalid(raw, e))?;
                paths.add(glob);
            } else {
                names.add(Glob::new(pattern).map_err(|e| invalid(raw, e))?);
            }
        }
        let build = |b: GlobSetBuilder| {
            b.build()
                .map_err(|e| ConfigError::Invalid(format!("invalid exclude patterns: {e}")))
        };
        Ok(Self {
            names: build(names)?,
            paths: build(paths)?,
        })
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| self.names.is_match(name))
            || self.paths.is_match(path)
    }
}

/// Manifest key for a path, if it can be transferred safely with smbclient
pub fn key_of(path: &Path) -> Option<String> {
    path.to_str()
        .filter(|s| is_safe_for_smbclient(s))
        .map(String::from)
}

/// Recursively scan `sources` (absolute paths). Symbolic links are not followed.
/// `progress` is called regularly with the number of files and bytes found so far.
pub fn scan(
    sources: &[PathBuf],
    excludes: &Excludes,
    max_file_size: u64,
    progress: &dyn Fn(u64, u64),
) -> ScanReport {
    let mut report = ScanReport::default();

    for source in sources {
        let meta = match fs::symlink_metadata(source) {
            Ok(meta) => meta,
            Err(e) => {
                report.skip(source, format!("cannot read: {e}"));
                continue;
            }
        };
        if !meta.is_dir() {
            report.consider(source.clone(), meta.file_type(), Some(meta), max_file_size);
            continue;
        }

        let mut stack = vec![source.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    report.skip(&dir, format!("cannot read directory: {e}"));
                    continue;
                }
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(e) => {
                        report.skip(&dir, format!("cannot read directory entry: {e}"));
                        continue;
                    }
                };
                let path = entry.path();
                if excludes.is_excluded(&path) {
                    debug!("Excluded: {}", path.display());
                    continue;
                }
                match entry.file_type() {
                    Ok(ft) if ft.is_dir() => {
                        if is_cache_dir(&path) {
                            debug!("Excluded cache folder (CACHEDIR.TAG): {}", path.display());
                        } else if is_python_venv(&path) {
                            debug!("Excluded Python virtual environment: {}", path.display());
                        } else if key_of(&path).is_some() {
                            stack.push(path);
                        } else {
                            report.skip(&path, UNSUPPORTED_NAME.to_string());
                        }
                    }
                    Ok(ft) => report.consider(path, ft, None, max_file_size),
                    Err(e) => report.skip(&path, format!("cannot read file type: {e}")),
                }
            }
            progress(report.files.len() as u64, report.total_size);
        }
    }

    // Overlapping sources (e.g. ~ and ~/Documents) must not upload files twice
    report.files.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    report.files.dedup_by(|a, b| a.key == b.key);
    report.total_size = report.files.iter().map(|f| f.size).sum();
    progress(report.files.len() as u64, report.total_size);
    report
}

impl ScanReport {
    fn skip(&mut self, path: &Path, reason: String) {
        debug!("Skipped {}: {reason}", path.display());
        self.skipped.push(FileIssue {
            path: path.to_string_lossy().into_owned(),
            reason,
        });
    }

    fn consider(
        &mut self,
        path: PathBuf,
        file_type: FileType,
        meta: Option<Metadata>,
        max_file_size: u64,
    ) {
        if file_type.is_symlink() {
            return self.skip(&path, "symbolic link (not followed)".into());
        }
        if !file_type.is_file() {
            return self.skip(&path, "special file (socket, FIFO or device)".into());
        }
        let Some(key) = key_of(&path) else {
            return self.skip(&path, UNSUPPORTED_NAME.into());
        };
        let meta = match meta.map_or_else(|| fs::symlink_metadata(&path), Ok) {
            Ok(meta) => meta,
            Err(e) => return self.skip(&path, format!("cannot read metadata: {e}")),
        };
        if max_file_size > 0 && meta.len() > max_file_size {
            return self.skip(&path, "larger than backup.max_file_size".into());
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_secs()).ok());
        self.total_size += meta.len();
        self.files.push(ScannedFile {
            path,
            key,
            size: meta.len(),
            mtime,
            mode: meta.permissions().mode() & 0o7777,
        });
    }

    /// Skipped entries grouped by reason, most frequent first
    pub fn skipped_by_reason(&self) -> Vec<(String, usize)> {
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for issue in &self.skipped {
            let reason = issue.reason.split(':').next().unwrap_or(&issue.reason);
            *counts.entry(reason).or_default() += 1;
        }
        let mut grouped: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(r, n)| (r.to_string(), n))
            .collect();
        grouped.sort_by_key(|g| std::cmp::Reverse(g.1));
        grouped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn excludes(patterns: &[&str]) -> Excludes {
        Excludes::new(&patterns.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn exclude_patterns() {
        let ex = excludes(&[".cache", "*.tmp", ".local/share/Trash", "/opt/data/**"]);
        assert!(ex.is_excluded(Path::new("/home/u/.cache")));
        assert!(ex.is_excluded(Path::new("/home/u/a/b.tmp")));
        assert!(ex.is_excluded(Path::new("/home/u/.local/share/Trash")));
        assert!(ex.is_excluded(Path::new("/opt/data/x/y")));
        assert!(!ex.is_excluded(Path::new("/home/u/.cache-not")));
        assert!(!ex.is_excluded(Path::new("/home/u/targets.txt")));
        assert!(!ex.is_excluded(Path::new("/home/u/share/Trash")));
    }

    #[test]
    fn firefox_site_cache_is_pruned_without_excluding_site_data() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let profile = root.join("firefox/profile");
        let site = profile.join("storage/default/https+++example.com");
        fs::create_dir_all(site.join("cache/morgue/101")).unwrap();
        fs::create_dir_all(site.join("idb")).unwrap();
        fs::write(site.join("cache/morgue/101/entry.final"), b"cache").unwrap();
        fs::write(site.join("idb/data.sqlite"), b"site data").unwrap();
        fs::write(profile.join("places.sqlite"), b"bookmarks").unwrap();
        let pattern = format!("{}/firefox/*/storage/default/*/cache", root.display());
        let ex = Excludes::new(&[pattern]).unwrap();
        let report = scan(&[root.to_path_buf()], &ex, 0, &|_, _| {});

        assert!(report.skipped.is_empty());
        let paths: Vec<_> = report.files.iter().map(|file| &file.path).collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&&site.join("idb/data.sqlite")));
        assert!(paths.contains(&&profile.join("places.sqlite")));
    }

    #[test]
    fn invalid_pattern_is_reported() {
        assert!(Excludes::new(&["a[".to_string()]).is_err());
    }

    #[test]
    fn scans_files_dirs_and_skips_specials() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/node_modules")).unwrap();
        fs::create_dir_all(root.join("a/target/debug")).unwrap();
        fs::write(
            root.join("a/target/CACHEDIR.TAG"),
            [CACHEDIR_TAG_SIGNATURE, b"\n# cargo"].concat(),
        )
        .unwrap();
        fs::write(root.join("a/target/debug/app"), b"bin").unwrap();
        fs::create_dir_all(root.join("a/myenv42/lib")).unwrap();
        fs::write(root.join("a/myenv42/pyvenv.cfg"), b"home = /usr/bin").unwrap();
        fs::write(root.join("a/myenv42/lib/site.py"), b"py").unwrap();
        fs::write(root.join("a/one.txt"), b"1").unwrap();
        fs::write(root.join("a/node_modules/x.js"), b"x").unwrap();
        fs::write(root.join("two.txt"), b"22").unwrap();
        fs::write(root.join("big.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("bad\"name"), b"q").unwrap();
        symlink(root.join("two.txt"), root.join("link")).unwrap();

        let ex = excludes(&["node_modules"]);
        // the second source overlaps the first one: files must not be duplicated
        let sources = vec![root.to_path_buf(), root.join("a")];
        let report = scan(&sources, &ex, 50, &|_, _| {});

        let names: Vec<&str> = report
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["one.txt", "two.txt"]);
        assert_eq!(report.total_size, 3);
        let reasons: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(reasons.len(), 3, "{reasons:?}");
        assert!(reasons.contains(&"symbolic link (not followed)"));
        assert!(reasons.contains(&"larger than backup.max_file_size"));
        assert!(reasons.contains(&UNSUPPORTED_NAME));
    }

    #[test]
    fn single_file_source() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("doc.pdf");
        fs::write(&file, b"pdf").unwrap();
        let report = scan(std::slice::from_ref(&file), &excludes(&[]), 0, &|_, _| {});
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].key, file.to_str().unwrap());
    }
}
