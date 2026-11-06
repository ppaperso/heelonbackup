//! Authenticated archives of sensitive sources, shared by the CLI and console.
//! Plaintext flows directly from tar through gzip into age; only ciphertext reaches disk.

use super::{Exit, expand_tilde};
use crate::config::{Config, PASSWORD_ENV, SealSecret, SealedConfig, SealedItem};
use crate::error::{BackupError, Result};
use crate::ui;
use age::secrecy::SecretString;
use flate2::{Compression, write::GzEncoder};
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Add or update one sealed item while preserving every other configured source.
pub(crate) fn configure_item(
    config: &Config,
    path: String,
    archive: String,
    staging_dir: String,
) -> Result<Config> {
    let mut updated = config.clone();
    let sealed = updated.sealed.get_or_insert_with(|| SealedConfig {
        staging_dir: staging_dir.clone(),
        secret: SealSecret::default(),
        items: Vec::new(),
    });
    sealed.staging_dir = staging_dir;
    let source = expand_tilde(&path)
        .canonicalize()
        .map_err(|e| BackupError::io("cannot resolve sensitive source", e))?;
    let existing = sealed.items.iter().position(|item| {
        expand_tilde(&item.path)
            .canonicalize()
            .is_ok_and(|candidate| candidate == source)
    });
    let item = SealedItem { path, archive };
    if let Some(index) = existing {
        sealed.items[index] = item;
    } else {
        sealed.items.push(item);
    }
    Plan::new(sealed)?;
    updated.validate()?;
    Ok(updated)
}

/// Create a partial request selecting only staging, even before the first archive exists.
pub(crate) fn backup_request(
    config: &Config,
    dry_run: bool,
) -> Result<super::backup::BackupRequest> {
    let sealed = config
        .sealed
        .as_ref()
        .ok_or_else(|| BackupError::Seal("configure sealed sources first".into()))?;
    let plan = Plan::new(sealed)?;
    Ok(super::backup::BackupRequest {
        paths: vec![plan.staging],
        dry_run,
        ..Default::default()
    })
}

/// Store a confirmed recovery passphrase through stdin, never process arguments.
pub(crate) async fn store_passphrase(config: &Config, value: SecretString) -> Result<Exit> {
    let sealed = config
        .sealed
        .as_ref()
        .ok_or_else(|| BackupError::Seal("configure sealed sources first".into()))?;
    sealed.validate()?;
    use age::secrecy::ExposeSecret;
    if value.expose_secret().trim().is_empty() {
        return Err(BackupError::Seal(
            "seal passphrase must not be empty".into(),
        ));
    }
    let child = Command::new("secret-tool")
        .args([
            "store",
            "--label=heelonbackup seal",
            &sealed.secret.attribute,
            &sealed.secret.key,
        ])
        .env_remove(PASSPHRASE_ENV)
        .env_remove(PASSWORD_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| BackupError::io("cannot run secret-tool store", e))?;
    tokio::select! {
        () = super::interrupted() => return Err(BackupError::Interrupted),
        result = tokio::time::timeout(Duration::from_secs(15), finish_keyring_store(child, value)) => {
            result.map_err(|_| BackupError::Seal("keyring store timed out; unlock the session keyring".into()))??;
        }
    }
    ui::say!(
        "Recovery passphrase stored in the keyring. Keep an independent offline copy; old archives still require their original passphrase."
    );
    Ok(Exit::Success)
}

async fn finish_keyring_store(mut child: tokio::process::Child, value: SecretString) -> Result<()> {
    use age::secrecy::ExposeSecret;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| BackupError::Seal("keyring process has no input pipe".into()))?;
    stdin
        .write_all(value.expose_secret().as_bytes())
        .await
        .map_err(|e| BackupError::io("cannot send recovery passphrase to keyring", e))?;
    stdin
        .shutdown()
        .await
        .map_err(|e| BackupError::io("cannot close keyring input", e))?;
    drop(stdin);
    let status = child
        .wait()
        .await
        .map_err(|e| BackupError::io("cannot wait for keyring store", e))?;
    if !status.success() {
        return Err(BackupError::Seal(
            "keyring store failed; unlock the session keyring and try again".into(),
        ));
    }
    Ok(())
}

/// Explicit override for unattended runs; never passed to child processes.
pub(crate) const PASSPHRASE_ENV: &str = "HEELONBACKUP_SEAL_PASSPHRASE";

#[derive(Debug, Clone)]
struct Item {
    path: PathBuf,
    archive: PathBuf,
}

/// Validated paths, resolved before scanning so missing sensitive sources fail closed.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub(crate) staging: PathBuf,
    items: Vec<Item>,
    config: SealedConfig,
}

/// Canonicalize the existing ancestor of a destination that may not exist yet.
fn destination(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return path
                .canonicalize()
                .map_err(|e| BackupError::io("cannot resolve sealing destination", e));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(BackupError::io("cannot inspect sealing destination", e)),
    }
    let parent = path.parent().ok_or_else(|| {
        BackupError::Seal("staging directory must have an existing ancestor".into())
    })?;
    let name = path
        .file_name()
        .ok_or_else(|| BackupError::Seal("invalid staging directory".into()))?;
    Ok(destination(parent)?.join(name))
}

impl Plan {
    pub(crate) fn new(config: &SealedConfig) -> Result<Self> {
        config.validate()?;
        let requested = expand_tilde(&config.staging_dir);
        match fs::symlink_metadata(&requested) {
            Ok(metadata) => check_staging(&metadata)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(BackupError::io(
                    "cannot inspect sealed staging directory",
                    e,
                ));
            }
        }
        let staging = destination(&requested)?;
        let mut items = Vec::new();
        for item in &config.items {
            let path = expand_tilde(&item.path).canonicalize().map_err(|e| {
                BackupError::io(format!("cannot resolve sealed source {}", item.path), e)
            })?;
            if !path.is_dir() && !path.is_file() {
                return Err(BackupError::Seal(
                    "sealed sources must be regular files or directories".into(),
                ));
            }
            if staging.starts_with(&path) || path.starts_with(&staging) {
                return Err(BackupError::Seal(
                    "staging directory and sealed sources must not overlap".into(),
                ));
            }
            items.push(Item {
                path,
                archive: staging.join(&item.archive),
            });
        }
        Ok(Self {
            staging,
            items,
            config: config.clone(),
        })
    }

    /// A full backup includes archives automatically. Partial runs include them only
    /// when the requested paths overlap staging or a sensitive source.
    pub(crate) fn includes_archives(&self, sources: &[PathBuf], full: bool) -> bool {
        full || sources.iter().any(|source| {
            source.starts_with(&self.staging)
                || self.staging.starts_with(source)
                || self
                    .items
                    .iter()
                    .any(|item| source.starts_with(&item.path) || item.path.starts_with(source))
        })
    }

    pub(crate) fn protected_paths(&self) -> Vec<PathBuf> {
        self.items
            .iter()
            .map(|i| i.path.clone())
            .chain([self.staging.clone()])
            .collect()
    }

    pub(crate) fn archives(&self) -> Vec<PathBuf> {
        self.items.iter().map(|i| i.archive.clone()).collect()
    }

    /// Allow staging to be selected before the first seal has created it.
    pub(crate) fn pending_source(&self, path: &Path) -> Option<PathBuf> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(path)
        };
        // Resolve directory aliases, but never follow an archive's final symlink:
        // it will be replaced by ciphertext before the scan.
        let resolved = absolute
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .zip(absolute.file_name())
            .map(|(parent, name)| parent.join(name));
        let path = resolved.as_deref().unwrap_or(&absolute);
        let expanded = expand_tilde(&self.config.staging_dir);
        if path == expanded || path == self.staging {
            Some(self.staging.clone())
        } else {
            self.items
                .iter()
                .find(|item| {
                    path == item.archive
                        || item
                            .archive
                            .file_name()
                            .is_some_and(|name| path == expanded.join(name))
                })
                .map(|item| item.archive.clone())
        }
    }
}

/// Holds the staging lock until the caller has finished scanning and uploading.
pub(crate) struct Prepared {
    _lock: File,
}

pub(crate) fn print_plan(plan: &Plan) {
    ui::say!("Sealed sources (age authenticated encryption):");
    for item in &plan.items {
        ui::say!("  {} -> {}", item.path.display(), item.archive.display());
    }
    ui::say!("  Sensitive paths and unlisted staging files are excluded from plaintext backup.");
}

pub(crate) async fn run(config: &crate::config::Config, dry_run: bool) -> Result<Exit> {
    let sealed = config.sealed.as_ref().ok_or_else(|| {
        BackupError::Seal("configure sealed.items and sealed.staging_dir first".into())
    })?;
    let plan = Plan::new(sealed)?;
    print_plan(&plan);
    if dry_run {
        ui::say!("Dry run: no keyring access, archive creation or NAS connection.");
    } else {
        let _prepared = create(plan).await?;
    }
    Ok(Exit::Success)
}

fn secret(value: String) -> Result<SecretString> {
    let value = SecretString::from(value);
    use age::secrecy::ExposeSecret;
    if value.expose_secret().trim().is_empty() {
        return Err(BackupError::Seal(
            "seal passphrase must not be empty".into(),
        ));
    }
    Ok(value)
}

async fn passphrase(config: &SealedConfig) -> Result<SecretString> {
    match std::env::var(PASSPHRASE_ENV) {
        Ok(value) => return secret(value),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(BackupError::Seal(
                "seal passphrase environment variable must be UTF-8".into(),
            ));
        }
        Err(std::env::VarError::NotPresent) => {}
    }
    let lookup = Command::new("secret-tool")
        .args(["lookup", &config.secret.attribute, &config.secret.key])
        .env_remove(PASSPHRASE_ENV)
        .env_remove(PASSWORD_ENV)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::select! {
        () = super::interrupted() => return Err(BackupError::Interrupted),
        result = tokio::time::timeout(Duration::from_secs(15), lookup) => {
            result.map_err(|_| BackupError::Seal(
                "keyring lookup timed out; unlock the session keyring or set HEELONBACKUP_SEAL_PASSPHRASE".into()
            ))?.map_err(|e| BackupError::io("cannot run secret-tool lookup", e))?
        }
    };
    if !output.status.success() {
        return Err(BackupError::Seal(
            "keyring lookup failed; unlock the session keyring or set HEELONBACKUP_SEAL_PASSPHRASE"
                .into(),
        ));
    }
    // Do not include stdout/stderr in errors: either could contain secret material.
    let mut value = String::from_utf8(output.stdout)
        .map_err(|_| BackupError::Seal("keyring passphrase must be UTF-8".into()))?;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    secret(value)
}

/// Seal every item before a backup; cancellation waits for the worker to clean up.
pub(crate) async fn create(plan: Plan) -> Result<Prepared> {
    let passphrase = passphrase(&plan.config).await?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let spinner = ui::spinner("Sealing sensitive sources...");
    let mut job = tokio::task::spawn_blocking(move || prepare(&plan, passphrase, &flag));
    let result = tokio::select! {
        result = &mut job => result.map_err(|e| BackupError::io("sealing worker stopped unexpectedly", io::Error::other(e))).and_then(|result| result),
        () = super::interrupted() => {
            cancelled.store(true, Ordering::SeqCst);
            // spawn_blocking cannot be aborted: do not return while it can still publish files.
            match job.await {
                Ok(_) => Err(BackupError::Interrupted),
                Err(e) => Err(BackupError::io("sealing worker stopped unexpectedly", io::Error::other(e))),
            }
        }
    };
    spinner.finish_and_clear();
    let prepared = result?;
    ui::say!("Sealed archives ready.");
    Ok(prepared)
}

fn check_staging(metadata: &fs::Metadata) -> Result<()> {
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(BackupError::Seal(
            "staging must be a private directory (run chmod 700 on sealed.staging_dir)".into(),
        ));
    }
    Ok(())
}

fn lock_staging(path: &Path) -> Result<File> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|e| BackupError::io("cannot create sealed staging directory", e))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| BackupError::io("cannot inspect sealed staging directory", e))?;
    check_staging(&metadata)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path.join(".seal.lock"))
        .map_err(|e| BackupError::io("cannot open sealed staging lock", e))?;
    if !file
        .metadata()
        .map_err(|e| BackupError::io("cannot inspect sealed staging lock", e))?
        .is_file()
    {
        return Err(BackupError::Seal(
            "staging lock must be a regular file".into(),
        ));
    }
    file.try_lock().map_err(|e| {
        BackupError::Seal(format!(
            "cannot lock staging (another seal or backup may be running): {e}"
        ))
    })?;
    Ok(file)
}

struct Cancellable<W> {
    inner: W,
    cancelled: Arc<AtomicBool>,
}

impl<W: Write> Write for Cancellable<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(io::Error::other("sealing cancelled"));
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn prepare(plan: &Plan, passphrase: SecretString, cancelled: &Arc<AtomicBool>) -> Result<Prepared> {
    let lock = lock_staging(&plan.staging)?;
    let mut pending = Vec::new();
    for item in &plan.items {
        let tmp = NamedTempFile::new_in(&plan.staging)
            .map_err(|e| BackupError::io("cannot create temporary encrypted archive", e))?;
        let encryptor = age::Encryptor::with_user_passphrase(passphrase.clone());
        let encrypted = encryptor
            .wrap_output(tmp.as_file())
            .map_err(|e| BackupError::Seal(format!("cannot initialize age encryption: {e}")))?;
        let compressed = GzEncoder::new(encrypted, Compression::default());
        let mut archive = tar::Builder::new(Cancellable {
            inner: compressed,
            cancelled: cancelled.clone(),
        });
        archive.follow_symlinks(false);
        let name = item
            .path
            .file_name()
            .ok_or_else(|| BackupError::Seal("cannot archive a filesystem root".into()))?;
        let archived = if item.path.is_dir() {
            archive.append_dir_all(name, &item.path)
        } else {
            archive.append_path_with_name(&item.path, name)
        };
        archived
            .map_err(|e| BackupError::io(format!("cannot archive {}", item.path.display()), e))?;
        archive
            .into_inner()
            .and_then(|writer| writer.inner.finish())
            .and_then(|writer| writer.finish())
            .map_err(|e| BackupError::io("cannot finish encrypted archive", e))?;
        tmp.as_file()
            .sync_all()
            .map_err(|e| BackupError::io("cannot sync encrypted archive", e))?;
        pending.push((tmp, &item.archive));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err(BackupError::Interrupted);
    }
    // Keep previous archives untouched until every new archive has been completed.
    for (tmp, path) in pending {
        tmp.persist(path)
            .map_err(|e| BackupError::io("cannot publish encrypted archive", e.error))?;
    }
    File::open(&plan.staging)
        .and_then(|f| f.sync_all())
        .map_err(|e| BackupError::io("cannot sync sealed staging directory", e))?;
    Ok(Prepared { _lock: lock })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, SealSecret, SealedItem};
    use std::io::Read;
    use std::os::unix::fs::symlink;

    fn fixture() -> (tempfile::TempDir, SealedConfig) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(".ssh");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("id_test"), b"synthetic private key\n").unwrap();
        fs::set_permissions(source.join("id_test"), fs::Permissions::from_mode(0o600)).unwrap();
        let config = SealedConfig {
            staging_dir: dir.path().join("sealed").to_str().unwrap().into(),
            secret: SealSecret::default(),
            items: vec![SealedItem {
                path: source.to_str().unwrap().into(),
                archive: "ssh.tar.gz.age".into(),
            }],
        };
        (dir, config)
    }

    #[test]
    fn configuration_adds_and_updates_without_losing_existing_items_or_sources() {
        let (dir, sealed) = fixture();
        let mut config = Config {
            sealed: Some(sealed.clone()),
            ..Config::default()
        };
        config.backup.sources = vec![dir.path().to_string_lossy().into_owned()];
        let original_sources = config.backup.sources.clone();
        let extra = dir.path().join("other-keys");
        fs::create_dir(&extra).unwrap();
        let added = configure_item(
            &config,
            extra.to_string_lossy().into_owned(),
            "other.tar.gz.age".into(),
            sealed.staging_dir.clone(),
        )
        .unwrap();
        assert_eq!(added.sealed.as_ref().unwrap().items.len(), 2);
        assert_eq!(added.backup.sources, original_sources);
        let updated = configure_item(
            &added,
            sealed.items[0].path.clone(),
            "renamed.tar.gz.age".into(),
            sealed.staging_dir.clone(),
        )
        .unwrap();
        let items = &updated.sealed.as_ref().unwrap().items;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].archive, "renamed.tar.gz.age");
        assert_eq!(items[1].archive, "other.tar.gz.age");
        assert!(
            configure_item(
                &config,
                extra.to_string_lossy().into_owned(),
                "ssh.tar.gz.age".into(),
                sealed.staging_dir.clone()
            )
            .is_err()
        );
        assert_eq!(config.sealed.as_ref().unwrap().items.len(), 1);
        assert!(!Path::new(&sealed.staging_dir).exists());
    }

    #[test]
    fn sealed_only_request_works_before_staging_exists_and_is_partial() {
        let (_dir, sealed) = fixture();
        let config = Config {
            sealed: Some(sealed.clone()),
            ..Config::default()
        };
        let request = backup_request(&config, true).unwrap();
        assert_eq!(request.paths, vec![PathBuf::from(&sealed.staging_dir)]);
        assert!(request.dry_run);
        assert!(!Path::new(&sealed.staging_dir).exists());
        assert!(!backup_request(&config, false).unwrap().dry_run);
        assert!(backup_request(&Config::default(), true).is_err());
    }

    #[tokio::test]
    async fn keyring_transport_uses_stdin_and_reports_failure_without_secret_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic-input");
        for status in ["0", "1"] {
            let child = Command::new("sh")
                .args([
                    "-c",
                    "cat > \"$1\"; printf 'sensitive-output' >&2; exit \"$2\"",
                    "test",
                ])
                .arg(&path)
                .arg(status)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let result =
                finish_keyring_store(child, SecretString::from("synthetic secret".to_owned()))
                    .await;
            assert_eq!(fs::read(&path).unwrap(), b"synthetic secret");
            if status == "0" {
                assert!(result.is_ok());
            } else {
                let error = result.unwrap_err().to_string();
                assert!(error.contains("keyring store failed"));
                assert!(!error.contains("sensitive-output"));
                assert!(!error.contains("synthetic secret"));
            }
        }
    }

    fn decrypt(
        bytes: &[u8],
        password: &str,
    ) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
        let decryptor = age::Decryptor::new(bytes)?;
        let identity = age::scrypt::Identity::new(SecretString::from(password.to_owned()));
        let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))?;
        let mut plaintext = Vec::new();
        reader.read_to_end(&mut plaintext)?;
        Ok(plaintext)
    }

    #[test]
    fn authenticated_roundtrip_permissions_and_locking() {
        let (dir, config) = fixture();
        let mut state = 42u32;
        let large: Vec<u8> = (0..200_000)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 24) as u8
            })
            .collect();
        let large_path = Path::new(&config.items[0].path).join("large");
        fs::write(&large_path, &large).unwrap();
        symlink("id_test", Path::new(&config.items[0].path).join("link")).unwrap();
        let plan = Plan::new(&config).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let prepared =
            prepare(&plan, secret("test passphrase".into()).unwrap(), &cancelled).unwrap();
        let path = &plan.items[0].archive;
        assert_eq!(
            fs::metadata(&plan.staging).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(lock_staging(&plan.staging).is_err());
        let encrypted = fs::read(path).unwrap();
        assert!(
            encrypted.len() > 64 * 1024,
            "exercise multiple authenticated age chunks"
        );
        assert!(
            !encrypted
                .windows(b"synthetic private key".len())
                .any(|b| b == b"synthetic private key")
        );
        let plaintext = decrypt(&encrypted, "test passphrase").unwrap();
        let restore = dir.path().join("restored");
        fs::create_dir(&restore).unwrap();
        tar::Archive::new(flate2::read::GzDecoder::new(&plaintext[..]))
            .unpack(&restore)
            .unwrap();
        assert_eq!(fs::read(restore.join(".ssh/large")).unwrap(), large);
        assert_eq!(
            fs::read_link(restore.join(".ssh/link")).unwrap(),
            Path::new("id_test")
        );
        assert_eq!(
            fs::read(restore.join(".ssh/id_test")).unwrap(),
            b"synthetic private key\n"
        );
        assert_eq!(
            fs::metadata(restore.join(".ssh/id_test"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(decrypt(&encrypted, "wrong passphrase").is_err());
        let mut changed = encrypted.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(decrypt(&changed, "test passphrase").is_err());
        assert!(decrypt(&encrypted[..encrypted.len() - 1], "test passphrase").is_err());
        assert_eq!(fs::read_dir(&plan.staging).unwrap().count(), 2);
        drop(prepared);
        assert!(lock_staging(&plan.staging).is_ok());
    }

    #[test]
    fn failed_or_cancelled_seal_keeps_old_archive_and_removes_temporary_files() {
        let (_dir, mut config) = fixture();
        let extra = PathBuf::from(&config.items[0].path).join("extra");
        fs::write(&extra, b"extra").unwrap();
        config.items.push(SealedItem {
            path: extra.to_str().unwrap().into(),
            archive: "extra.tar.gz.age".into(),
        });
        let plan = Plan::new(&config).unwrap();
        drop(lock_staging(&plan.staging).unwrap());
        fs::write(&plan.items[0].archive, b"previous ciphertext").unwrap();
        fs::remove_file(extra).unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        assert!(prepare(&plan, secret("test".into()).unwrap(), &flag).is_err());
        assert_eq!(
            fs::read(&plan.items[0].archive).unwrap(),
            b"previous ciphertext"
        );
        assert_eq!(fs::read_dir(&plan.staging).unwrap().count(), 2);
        flag.store(true, Ordering::SeqCst);
        assert!(prepare(&plan, secret("test".into()).unwrap(), &flag).is_err());
        assert_eq!(
            fs::read(&plan.items[0].archive).unwrap(),
            b"previous ciphertext"
        );
        assert_eq!(fs::read_dir(&plan.staging).unwrap().count(), 2);
    }

    #[test]
    fn plan_rejects_missing_sources_overlap_and_symlink_staging() {
        let (dir, mut config) = fixture();
        let original = config.clone();
        config.items[0].path = dir.path().join("missing").to_str().unwrap().into();
        assert!(Plan::new(&config).is_err());
        config = original.clone();
        config.staging_dir = format!("{}/sealed", config.items[0].path);
        assert!(Plan::new(&config).is_err());
        config.staging_dir = dir.path().to_str().unwrap().into();
        assert!(Plan::new(&config).is_err());
        config = original;
        symlink(dir.path(), &config.staging_dir).unwrap();
        assert!(Plan::new(&config).is_err());
    }

    #[test]
    fn unsafe_staging_permissions_are_rejected() {
        let (_dir, config) = fixture();
        fs::create_dir(&config.staging_dir).unwrap();
        fs::set_permissions(&config.staging_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(lock_staging(Path::new(&config.staging_dir)).is_err());
        assert!(Plan::new(&config).is_err());
    }

    #[test]
    fn staging_lock_cannot_follow_a_symlink() {
        let (dir, config) = fixture();
        let plan = Plan::new(&config).unwrap();
        fs::create_dir(&plan.staging).unwrap();
        fs::set_permissions(&plan.staging, fs::Permissions::from_mode(0o700)).unwrap();
        let target = dir.path().join("target");
        fs::write(&target, b"unchanged").unwrap();
        symlink(&target, plan.staging.join(".seal.lock")).unwrap();
        assert!(lock_staging(&plan.staging).is_err());
        assert_eq!(fs::read(target).unwrap(), b"unchanged");
    }

    #[test]
    fn partial_runs_include_archives_only_for_relevant_paths() {
        let (dir, config) = fixture();
        let plan = Plan::new(&config).unwrap();
        assert!(plan.includes_archives(&[], true));
        assert!(plan.includes_archives(&[dir.path().to_path_buf()], false));
        assert!(plan.includes_archives(&[plan.items[0].path.join("id_test")], false));
        assert!(plan.includes_archives(std::slice::from_ref(&plan.staging), false));
        assert!(!plan.includes_archives(&[dir.path().join("documents")], false));
        assert_eq!(
            plan.pending_source(Path::new(&config.staging_dir)),
            Some(plan.staging.clone())
        );
    }

    #[tokio::test]
    async fn dry_run_never_creates_staging_or_reads_keyring() {
        let (_dir, sealed) = fixture();
        let config = Config {
            sealed: Some(sealed.clone()),
            ..Config::default()
        };
        assert_eq!(run(&config, true).await.unwrap(), Exit::Success);
        assert!(!Path::new(&sealed.staging_dir).exists());
        assert!(run(&Config::default(), true).await.is_err());
    }

    #[test]
    fn passphrase_validation_and_debug_do_not_expose_secrets() {
        assert!(secret(String::new()).is_err());
        assert!(secret(" \t\n".into()).is_err());
        assert!(
            !format!("{:?}", secret("synthetic secret".into()).unwrap())
                .contains("synthetic secret")
        );
    }
}
