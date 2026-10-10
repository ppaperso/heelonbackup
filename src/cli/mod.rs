//! Command line interface

mod backup;
mod common;
mod config_cmd;
mod restore;
mod status;
mod verify;

use crate::config::Config;
use crate::error::BackupError;
use crate::ui;
use clap::{CommandFactory, Parser, Subcommand};
use common::Exit;
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "heelonbackup", version, about, long_about = None)]
pub struct Cli {
    /// Show debug messages
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Configuration file [default: ~/.config/heelonbackup/config.json, or .heelonbackup/config.json with HEELONBACKUP_ENV=dev]
    #[arg(short, long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Back up the configured sources, or the given files/folders, to the NAS
    Backup(backup::BackupArgs),

    /// Show the result of the last backups made from this computer
    Status {
        /// Show every recorded backup
        #[arg(short, long)]
        all: bool,
        /// Show failed and skipped files
        #[arg(short, long)]
        detailed: bool,
    },

    /// List the backups stored on the NAS
    List,

    /// Check that a backup is complete on the NAS
    Verify {
        /// Backup name (YYYYMMDD_HHMMSS) [default: latest]
        backup: Option<String>,
        /// Download every file and check its SHA-256 checksum (slow)
        #[arg(long)]
        deep: bool,
    },

    /// Restore files from a backup (use --dry-run to only check what would happen)
    Restore(restore::RestoreArgs),

    /// Manage the configuration (without action: create it if missing, otherwise show it)
    Config {
        #[command(subcommand)]
        action: Option<config_cmd::ConfigAction>,
    },

    /// Generate shell completions
    Completions { shell: clap_complete::Shell },
}

/// Initialise logging once: --verbose > RUST_LOG > log_level from the configuration
pub(crate) fn init_logging(verbose: bool, config_level: Option<&str>) {
    let filter = if verbose {
        EnvFilter::new("heelonbackup=debug")
    } else if let Ok(filter) = EnvFilter::try_from_default_env() {
        filter
    } else {
        let level = config_level.unwrap_or("warn");
        EnvFilter::try_new(format!("heelonbackup={level}"))
            .unwrap_or_else(|_| EnvFilter::new("heelonbackup=warn"))
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(|| ui::LogWriter)
        .with_target(false)
        .without_time()
        .try_init();
}

pub async fn run() -> ExitCode {
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(Config::default_path);
    let verbose = cli.verbose;
    ui::print_environment_banner();

    let result = match cli.command {
        Commands::Backup(args) => match common::load_config(&config_path, verbose) {
            Ok(config) => backup::run(&config, args).await,
            Err(e) => Err(e),
        },
        Commands::Status { all, detailed } => {
            init_logging(verbose, None);
            Ok(status::status(all, detailed))
        }
        Commands::List => match common::load_config(&config_path, verbose) {
            Ok(config) => status::list(&config).await,
            Err(e) => Err(e),
        },
        Commands::Verify { backup, deep } => match common::load_config(&config_path, verbose) {
            Ok(config) => verify::run(&config, backup.as_deref(), deep).await,
            Err(e) => Err(e),
        },
        Commands::Restore(args) => match common::load_config(&config_path, verbose) {
            Ok(config) => restore::run(&config, args).await,
            Err(e) => Err(e),
        },
        Commands::Config { action } => {
            init_logging(verbose, None);
            config_cmd::run(&config_path, action)
        }
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "heelonbackup",
                &mut std::io::stdout(),
            );
            Ok(Exit::Success)
        }
    };

    match result {
        Ok(exit) => exit.into(),
        Err(BackupError::Interrupted) => {
            eprintln!("⏹  Interrupted");
            Exit::Interrupted.into()
        }
        Err(e) => {
            eprintln!("❌ Error: {e}");
            let mut source = std::error::Error::source(&e);
            while let Some(s) = source {
                eprintln!("   caused by: {s}");
                source = s.source();
            }
            Exit::Failure.into()
        }
    }
}
