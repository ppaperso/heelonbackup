//! `heelonbackup config`

use super::common::Exit;
use crate::config::{Config, PASSWORD_ENV, SmbTarget};
use crate::error::{BackupError, ConfigError, Result};
use clap::Subcommand;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::Command;

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Create the configuration file (interactive when run in a terminal)
    Init {
        /// Replace an existing configuration
        #[arg(long)]
        force: bool,
    },
    /// Print the configuration (password hidden)
    Show,
    /// Open the configuration in $VISUAL / $EDITOR, then validate it
    Edit,
    /// Print the path of the configuration file
    Path,
}

pub fn run(path: &Path, action: Option<ConfigAction>) -> Result<Exit> {
    let action = action.unwrap_or(if path.exists() {
        ConfigAction::Show
    } else {
        ConfigAction::Init { force: false }
    });
    match action {
        ConfigAction::Init { force } => init(path, force),
        ConfigAction::Show => show(path),
        ConfigAction::Edit => edit(path),
        ConfigAction::Path => {
            println!("{}", path.display());
            Ok(Exit::Success)
        }
    }
}

fn ask(question: &str, default: &str) -> Result<String> {
    print!("{question} [{default}]: ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| BackupError::io("cannot read answer", e))?;
    let line = line.trim();
    Ok(if line.is_empty() { default } else { line }.to_string())
}

fn init(path: &Path, force: bool) -> Result<Exit> {
    if path.exists() && !force {
        return Err(ConfigError::Invalid(format!(
            "{} already exists (use `config init --force` to replace it, or `config edit`)",
            path.display()
        ))
        .into());
    }
    let mut config = Config::default();
    if std::io::stdin().is_terminal() {
        println!("Creating {}", path.display());
        loop {
            let url = ask(
                "NAS share URL (smb://host[:port]/share[/folder])",
                &config.smb.url,
            )?;
            match SmbTarget::parse(&url) {
                Ok(_) => {
                    config.smb.url = url;
                    break;
                }
                Err(e) => println!("  {e}"),
            }
        }
        config.smb.username = ask("SMB user", &config.smb.username)?;
        let sources = ask(
            "Folders/files to back up (comma separated)",
            &config.backup.sources.join(","),
        )?;
        config.backup.sources = sources
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    config.validate()?;
    config.save(path)?;
    println!("✅ Configuration written to {} (mode 600)", path.display());
    println!(
        "   The password is not stored: it is asked at each run, or read from {PASSWORD_ENV}."
    );
    println!("   Next: `heelonbackup backup --dry-run` to check the setup.");
    Ok(Exit::Success)
}

fn show(path: &Path) -> Result<Exit> {
    let mut config = Config::load(path)?;
    if config.smb.password.is_some() {
        config.smb.password = Some("********".into());
    }
    println!("# {}", path.display());
    println!("{}", serde_json::to_string_pretty(&config)?);
    if let Some(warning) = Config::permission_warning(path, &Config::load(path)?) {
        println!("⚠️  {warning}");
    }
    Ok(Exit::Success)
}

fn edit(path: &Path) -> Result<Exit> {
    if !path.exists() {
        Config::default().save(path)?;
    }
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
        .filter(|e| !e.trim().is_empty());
    let candidates: Vec<Vec<String>> = match editor {
        Some(e) => vec![e.split_whitespace().map(String::from).collect()],
        None => vec![vec!["nano".into()], vec!["vi".into()]],
    };
    let mut launched = false;
    for cmd in candidates {
        match Command::new(&cmd[0]).args(&cmd[1..]).arg(path).status() {
            Ok(_) => {
                launched = true;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(BackupError::io(format!("cannot run {}", cmd[0]), e)),
        }
    }
    if !launched {
        return Err(ConfigError::Invalid("no editor found: set $EDITOR".into()).into());
    }
    match Config::load(path) {
        Ok(_) => {
            println!("✅ Configuration is valid");
            Ok(Exit::Success)
        }
        Err(e) => {
            println!("❌ {e}");
            if let Some(source) = std::error::Error::source(&e) {
                println!("   {source}");
            }
            Ok(Exit::Failure)
        }
    }
}
