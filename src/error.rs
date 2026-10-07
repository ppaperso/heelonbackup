//! Error types for HeelonBackup

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T, E = BackupError> = std::result::Result<T, E>;

/// Main error type for the backup application
#[derive(Debug, Error)]
pub enum BackupError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// The NAS could not be reached or the share does not exist
    #[error("cannot reach the NAS: {0}")]
    SmbConnection(String),

    #[error("SMB authentication failed: {0}")]
    SmbAuth(String),

    /// A command executed on the share failed
    #[error("SMB operation failed: {0}")]
    SmbProtocol(String),

    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid manifest")]
    Manifest(#[from] serde_json::Error),

    #[error("backup not found: {0}")]
    BackupNotFound(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("operation interrupted by user")]
    Interrupted,
}

impl BackupError {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// Configuration specific errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration file not found at {0} (run `heelonbackup config init` to create it)")]
    NotFound(PathBuf),

    #[error("cannot access configuration file {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid configuration file {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid SMB URL `{0}` (expected smb://host[:port]/share[/folder])")]
    InvalidSmbUrl(String),

    #[error("invalid configuration: {0}")]
    Invalid(String),
}
