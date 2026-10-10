//! Configuration management for HeelonBackup

use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{IsTerminal, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Environment variable that selects the development environment (`dev`)
pub const ENV_VAR: &str = "HEELONBACKUP_ENV";

/// Prefix required for `storage.base_dir` in the development environment
pub const DEV_BASE_DIR_PREFIX: &str = "dev_";

/// Development or production environment, kept apart so tests never touch real backups
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    /// `HEELONBACKUP_ENV=dev` (set by the justfile): config and history in the project
    Dev,
    /// Default: config in `~/.config`, history in `~/.local/share`
    Prod,
}

impl Environment {
    /// Current environment, from `HEELONBACKUP_ENV`
    pub fn detect() -> Self {
        match std::env::var(ENV_VAR).as_deref() {
            Ok("dev") => Self::Dev,
            _ => Self::Prod,
        }
    }

    /// Local folder holding the configuration (and, in dev, the history)
    fn config_dir(self) -> PathBuf {
        match self {
            // Anchored on the project, whatever the current directory
            Self::Dev => Path::new(env!("CARGO_MANIFEST_DIR")).join(".heelonbackup"),
            Self::Prod => dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from(".config"))
                .join("heelonbackup"),
        }
    }

    pub fn config_path(self) -> PathBuf {
        self.config_dir().join("config.json")
    }

    pub fn history_dir(self) -> PathBuf {
        match self {
            Self::Dev => self.config_dir().join("history"),
            Self::Prod => dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from(".local/share"))
                .join("heelonbackup")
                .join("history"),
        }
    }

    fn default_base_dir(self) -> String {
        match self {
            Self::Dev => format!("{DEV_BASE_DIR_PREFIX}heelonbackup"),
            Self::Prod => "heelonbackup".to_string(),
        }
    }

    /// Dev backups must go to a `dev_*` folder on the NAS, and production backups must not
    pub fn check_base_dir(self, base_dir: &str) -> Result<(), ConfigError> {
        let is_dev_dir = base_dir
            .trim_start_matches('/')
            .starts_with(DEV_BASE_DIR_PREFIX);
        match (self, is_dev_dir) {
            (Self::Dev, false) => Err(ConfigError::Invalid(format!(
                "storage.base_dir `{base_dir}` must start with `{DEV_BASE_DIR_PREFIX}` in the \
                 development environment ({ENV_VAR}=dev), to keep tests apart from real backups"
            ))),
            (Self::Prod, true) => Err(ConfigError::Invalid(format!(
                "storage.base_dir `{base_dir}` is a development folder; production backups \
                 must not start with `{DEV_BASE_DIR_PREFIX}`"
            ))),
            _ => Ok(()),
        }
    }
}

/// Environment variable that overrides the SMB password from the configuration
pub const PASSWORD_ENV: &str = "HEELONBACKUP_SMB_PASSWORD";

/// SMB connection configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct SmbConfig {
    /// SMB URL: smb://host[:port]/share[/folder]
    pub url: String,
    /// Username for authentication
    pub username: String,
    /// Password (optional: prefer the HEELONBACKUP_SMB_PASSWORD variable or the interactive prompt)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Workgroup/domain (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workgroup: Option<String>,
    /// Per-operation timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Require SMB3 encryption of the traffic
    #[serde(default = "default_true")]
    pub encrypt: bool,
}

impl fmt::Debug for SmbConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmbConfig")
            .field("url", &self.url)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("workgroup", &self.workgroup)
            .field("timeout", &self.timeout)
            .field("encrypt", &self.encrypt)
            .finish()
    }
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Files or directories to back up
    #[serde(default = "default_sources")]
    pub sources: Vec<String>,
    /// Exclude patterns (glob). Without `/` a pattern matches any file or directory name,
    /// with `/` it matches the end of the path (or the full path if it starts with `/`).
    #[serde(default)]
    pub excludes: Vec<String>,
    /// Maximum file size in bytes (0 = no limit)
    #[serde(default)]
    pub max_file_size: u64,
    /// Check that every uploaded file is present on the NAS with the expected size
    #[serde(default = "default_true")]
    pub verify: bool,
    /// Number of parallel SMB sessions
    #[serde(default = "default_workers")]
    pub workers: usize,
}

/// Storage configuration for backups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Base directory on the share
    #[serde(default = "default_base_dir")]
    pub base_dir: String,
    /// Number of backups to keep on the NAS (0 = keep everything)
    #[serde(default = "default_retention")]
    pub retention: usize,
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub smb: SmbConfig,
    #[serde(default)]
    pub backup: BackupConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    /// Log level for diagnostics (error, warn, info, debug, trace)
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_timeout() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

fn default_sources() -> Vec<String> {
    vec![
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/home"))
            .to_string_lossy()
            .into_owned(),
    ]
}

fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 4))
}

fn default_base_dir() -> String {
    Environment::detect().default_base_dir()
}

fn default_retention() -> usize {
    5
}

fn default_log_level() -> String {
    "warn".to_string()
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            sources: default_sources(),
            excludes: [
                ".cache",
                ".thumbnails",
                ".local/share/Trash",
                ".var/app/*/cache",
                "node_modules",
                "__pycache__",
                "*.tmp",
            ]
            .map(String::from)
            .to_vec(),
            max_file_size: 0,
            verify: true,
            workers: default_workers(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_dir: default_base_dir(),
            retention: default_retention(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            smb: SmbConfig {
                url: "smb://beestation.local/backup".to_string(),
                username: std::env::var("USER").unwrap_or_else(|_| "admin".to_string()),
                password: None,
                workgroup: None,
                timeout: default_timeout(),
                encrypt: true,
            },
            backup: BackupConfig::default(),
            storage: StorageConfig::default(),
            log_level: default_log_level(),
        }
    }
}

/// Parsed form of `smb://host[:port]/share[/folder]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbTarget {
    pub host: String,
    pub port: Option<u16>,
    pub share: String,
    /// Folder inside the share (components separated by `/`)
    pub folder: Vec<String>,
}

impl SmbTarget {
    pub fn parse(url: &str) -> Result<Self, ConfigError> {
        let invalid = || ConfigError::InvalidSmbUrl(url.to_string());
        let rest = url.strip_prefix("smb://").ok_or_else(invalid)?;
        let (authority, path) = rest.split_once('/').ok_or_else(invalid)?;

        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port.parse::<u16>().map_err(|_| invalid())?)),
            None => (authority, None),
        };
        if host.is_empty()
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(invalid());
        }

        let mut components = path.split('/').filter(|c| !c.is_empty());
        let share = components.next().ok_or_else(invalid)?.to_string();
        let folder: Vec<String> = components.map(String::from).collect();
        if !is_safe_component(&share) || !folder.iter().all(|c| is_safe_component(c)) {
            return Err(invalid());
        }

        Ok(Self {
            host: host.to_string(),
            port,
            share,
            folder,
        })
    }

    /// `//host/share` as expected by smbclient
    pub fn service(&self) -> String {
        format!("//{}/{}", self.host, self.share)
    }
}

impl fmt::Display for SmbTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "//{}", self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        write!(f, "/{}", self.share)
    }
}

/// True if `s` can safely be used inside a quoted smbclient command
/// (no quote, backslash, line break or control character).
pub fn is_safe_for_smbclient(s: &str) -> bool {
    !s.chars().any(|c| c == '"' || c == '\\' || c.is_control())
}

/// True if `s` is a single, safe remote path component
pub fn is_safe_component(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && is_safe_for_smbclient(s)
}

impl Config {
    /// Default configuration file path for the current environment
    pub fn default_path() -> PathBuf {
        Environment::detect().config_path()
    }

    /// Load and validate the configuration
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound(path.to_path_buf())
            } else {
                ConfigError::Io {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
        let config: Config =
            serde_json::from_str(&content).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        config.validate()?;
        Environment::detect().check_base_dir(&config.storage.base_dir)?;
        Ok(config)
    }

    /// Save the configuration atomically, readable by the owner only (it may hold a password)
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let io_err = |source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_err)?;
        }
        let content = serde_json::to_string_pretty(self).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;

        let tmp = path.with_extension("json.tmp");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(io_err)?;
        file.write_all(content.as_bytes()).map_err(io_err)?;
        file.write_all(b"\n").map_err(io_err)?;
        file.sync_all().map_err(io_err)?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(io_err)?;
        fs::rename(&tmp, path).map_err(io_err)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), ConfigError> {
        SmbTarget::parse(&self.smb.url)?;
        if self.smb.username.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "smb.username must not be empty".into(),
            ));
        }
        if self.smb.timeout == 0 {
            return Err(ConfigError::Invalid(
                "smb.timeout must be at least 1 second".into(),
            ));
        }
        if self.backup.sources.is_empty() {
            return Err(ConfigError::Invalid(
                "backup.sources must not be empty".into(),
            ));
        }
        if !(1..=16).contains(&self.backup.workers) {
            return Err(ConfigError::Invalid(
                "backup.workers must be between 1 and 16".into(),
            ));
        }
        self.remote_base()?;
        Ok(())
    }

    /// Remote directory (relative to the share root) that holds all backups
    pub fn remote_base(&self) -> Result<String, ConfigError> {
        let target = SmbTarget::parse(&self.smb.url)?;
        let base: Vec<&str> = self
            .storage
            .base_dir
            .split('/')
            .filter(|c| !c.is_empty())
            .collect();
        if base.is_empty() || !base.iter().all(|c| is_safe_component(c)) {
            return Err(ConfigError::Invalid(format!(
                "storage.base_dir `{}` is not a valid relative folder",
                self.storage.base_dir
            )));
        }
        let parts: Vec<&str> = target
            .folder
            .iter()
            .map(String::as_str)
            .chain(base)
            .collect();
        Ok(parts.join("/"))
    }

    /// Warning if the configuration contains a password but is readable by other users
    pub fn permission_warning(path: &Path, config: &Config) -> Option<String> {
        config.smb.password.as_ref()?;
        let mode = fs::metadata(path).ok()?.permissions().mode();
        (mode & 0o077 != 0).then(|| {
            format!(
                "{} contains a password but is readable by other users; run `chmod 600 {}`",
                path.display(),
                path.display()
            )
        })
    }

    /// Password from the environment, the configuration, or an interactive prompt
    pub fn resolve_password(&self) -> Result<String, ConfigError> {
        if let Ok(password) = std::env::var(PASSWORD_ENV) {
            return Ok(password);
        }
        if let Some(password) = &self.smb.password {
            return Ok(password.clone());
        }
        if std::io::stdin().is_terminal() {
            let target = SmbTarget::parse(&self.smb.url)?;
            return rpassword::prompt_password(format!(
                "SMB password for {}@{}: ",
                self.smb.username, target.host
            ))
            .map_err(|source| ConfigError::Io {
                path: PathBuf::from("/dev/tty"),
                source,
            });
        }
        Err(ConfigError::Invalid(format!(
            "no SMB password available: set {PASSWORD_ENV} or smb.password"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_smb_url() {
        let t = SmbTarget::parse("smb://192.168.1.10/backup").unwrap();
        assert_eq!(t.host, "192.168.1.10");
        assert_eq!(t.port, None);
        assert_eq!(t.share, "backup");
        assert!(t.folder.is_empty());
        assert_eq!(t.service(), "//192.168.1.10/backup");

        let t = SmbTarget::parse("smb://nas.local:4450/home/laptop/").unwrap();
        assert_eq!(t.port, Some(4450));
        assert_eq!(t.share, "home");
        assert_eq!(t.folder, vec!["laptop"]);
    }

    #[test]
    fn reject_invalid_urls() {
        for url in [
            "nas/backup",
            "smb://nas",
            "smb:///backup",
            "smb://nas:x/backup",
            "smb://nas/back\"up",
            "smb://nas/backup/../x",
            "smb://na s/backup",
        ] {
            assert!(SmbTarget::parse(url).is_err(), "{url} should be rejected");
        }
    }

    #[test]
    fn remote_base_joins_folder_and_base_dir() {
        let mut config = Config::default();
        config.smb.url = "smb://nas/share/laptop".into();
        config.storage.base_dir = "/heelonbackup/".into();
        assert_eq!(config.remote_base().unwrap(), "laptop/heelonbackup");

        config.storage.base_dir = "../escape".into();
        assert!(config.remote_base().is_err());
    }

    #[test]
    fn legacy_fields_are_ignored() {
        let json = r#"{"smb":{"url":"smb://nas/b","username":"u"},
            "backup":{"sources":["/tmp"],"compress":true,"compression_level":6}}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
        assert!(config.smb.encrypt);
    }

    #[test]
    fn base_dir_matches_environment() {
        assert!(Environment::Dev.check_base_dir("dev_heelonbackup").is_ok());
        assert!(
            Environment::Dev
                .check_base_dir("/dev_heelonbackup/")
                .is_ok()
        );
        assert!(Environment::Dev.check_base_dir("heelonbackup").is_err());
        assert!(Environment::Dev.check_base_dir("backups/dev_x").is_err());
        assert!(Environment::Prod.check_base_dir("heelonbackup").is_ok());
        assert!(Environment::Prod.check_base_dir("backup_devices").is_ok());
        assert!(
            Environment::Prod
                .check_base_dir("dev_heelonbackup")
                .is_err()
        );
        assert_eq!(
            Environment::Dev.history_dir().parent(),
            Some(Environment::Dev.config_path().parent().unwrap())
        );
    }

    #[test]
    fn debug_redacts_password() {
        let mut config = Config::default();
        config.smb.password = Some("secret".into());
        assert!(!format!("{config:?}").contains("secret"));
    }

    #[test]
    fn save_uses_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.json");
        Config::default().save(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(Config::load(&path).is_ok());
    }
}
