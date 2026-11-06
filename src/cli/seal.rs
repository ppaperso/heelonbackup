//! `heelonbackup seal`: prepare encrypted archives without connecting to the NAS.

use crate::config::Config;
use crate::error::Result;
use crate::service::{self, Exit};
use clap::Args;

#[derive(Args, Debug)]
pub struct SealArgs {
    /// Show the sealing plan without reading the keyring or writing archives
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    /// Store/replace the recovery passphrase in the keyring (masked confirmation)
    #[arg(long, conflicts_with = "dry_run")]
    pub set_passphrase: bool,
}

pub async fn run(config: &Config, args: SealArgs) -> Result<Exit> {
    if args.set_passphrase {
        use crate::error::BackupError;
        use age::secrecy::{ExposeSecret, SecretString};
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            return Err(BackupError::Seal(
                "setting the recovery passphrase requires an interactive terminal".into(),
            ));
        }
        crate::ui::say!(
            "WARNING: replacing the keyring passphrase does not change old archives. Preserve their original passphrase offline. HEELONBACKUP_SEAL_PASSPHRASE still overrides the keyring."
        );
        let first = SecretString::from(
            rpassword::prompt_password("Recovery passphrase: ")
                .map_err(|e| BackupError::io("cannot read recovery passphrase", e))?,
        );
        let second = SecretString::from(
            rpassword::prompt_password("Confirm recovery passphrase: ")
                .map_err(|e| BackupError::io("cannot confirm recovery passphrase", e))?,
        );
        if first.expose_secret() != second.expose_secret() {
            return Err(BackupError::Seal("passphrases do not match".into()));
        }
        return service::seal::store_passphrase(config, first).await;
    }
    service::seal::run(config, args.dry_run).await
}
