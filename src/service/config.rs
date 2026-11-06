//! Configuration actions shared by `heelonbackup config` (cli) and the console config flow

use super::Exit;
use crate::config::{Config, PASSWORD_ENV, SmbTarget};
use crate::error::{BackupError, ConfigError, Result};
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::Command;

/// Configuration action, without the clap parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Create the configuration file (interactive when run in a terminal)
    Init { force: bool },
    /// Print the configuration (password hidden)
    Show,
    /// Open the configuration in $VISUAL / $EDITOR, then validate it
    Edit,
    /// Print the path of the configuration file
    Path,
}

pub fn run(path: &Path, action: Option<Action>) -> Result<Exit> {
    let action = action.unwrap_or(if path.exists() {
        Action::Show
    } else {
        Action::Init { force: false }
    });
    match action {
        Action::Init { force } => init(path, force),
        Action::Show => show(path),
        Action::Edit => edit(path),
        Action::Path => {
            print_config_path(path);
            Ok(Exit::Success)
        }
    }
}

/// Read one line from stdin, `default` when empty
// TODO(console): in the console flow the questions go through dialoguer instead
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
        print_creating(path);
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
                Err(e) => print_url_error(&e),
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
    print_config_written(path);
    Ok(Exit::Success)
}

fn show(path: &Path) -> Result<Exit> {
    let mut config = Config::load(path)?;
    if config.smb.password.is_some() {
        config.smb.password = Some("********".into());
    }
    let json = serde_json::to_string_pretty(&config)?;
    let warning = Config::permission_warning(path, &Config::load(path)?);
    print_config(path, &json, warning.as_deref());
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
            print_config_valid();
            Ok(Exit::Success)
        }
        Err(e) => {
            print_config_invalid(&e);
            Ok(Exit::Failure)
        }
    }
}

// ---- Display ----
// TODO(console): all the printing of this module is grouped in the small
// functions below. In phase 3 the display will be passed as a parameter of
// run() so the console can render its own output; until then they keep the
// exact CLI output.

// TODO(console)
fn print_config_path(path: &Path) {
    println!("{}", path.display());
}

// TODO(console)
fn print_creating(path: &Path) {
    println!("Creating {}", path.display());
}

// TODO(console)
fn print_url_error(e: &ConfigError) {
    println!("  {e}");
}

// TODO(console)
fn print_config_written(path: &Path) {
    println!("✅ Configuration written to {} (mode 600)", path.display());
    println!(
        "   The password is not stored: it is asked at each run, or read from {PASSWORD_ENV}."
    );
    println!("   Next: `heelonbackup backup --dry-run` to check the setup.");
}

/// The configuration file content (password masked), with an optional permissions warning
// TODO(console)
fn print_config(path: &Path, json: &str, warning: Option<&str>) {
    println!("# {}", path.display());
    println!("{json}");
    if let Some(warning) = warning {
        println!("⚠️  {warning}");
    }
}

// TODO(console)
fn print_config_valid() {
    println!("✅ Configuration is valid");
}

// TODO(console)
fn print_config_invalid(e: &ConfigError) {
    println!("❌ {e}");
    if let Some(source) = std::error::Error::source(e) {
        println!("   {source}");
    }
}
