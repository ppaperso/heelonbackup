//! Remote side of the backup: SMB access, transfers and manifests

pub mod backup_manifest;
pub mod smb_client;
pub mod transfer;

pub use backup_manifest::{BackupManifest, BackupStatus, FileInfo, FileIssue, LocalHistory};
pub use smb_client::SmbClient;
