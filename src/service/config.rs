//! Configuration actions shared by `heelonbackup config` (cli) and the console config flow

use super::{Exit, expand_tilde};
use crate::backup::Excludes;
use crate::config::{Config, PASSWORD_ENV, SmbTarget};
use crate::error::{BackupError, ConfigError, Result};
use crate::ui;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
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
    /// Validate the configuration file and resolve the configured sources (no network access)
    Check,
    /// List the configured sources (found on disk or missing) and the exclude patterns
    Sources,
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
        Action::Check => check(path),
        Action::Sources => sources(path),
        Action::Path => {
            print_config_path(path);
            Ok(Exit::Success)
        }
    }
}

/// Validate the configuration file: parse + validate + exclude patterns + sources exist.
/// Prints a summary and a final verdict; no network access (use `list` for that).
fn check(path: &Path) -> Result<Exit> {
    let config = Config::load(path)?; // parse + validate + environment base_dir check
    Excludes::new(&config.backup.excludes)?; // exclude patterns must compile
    let (found, missing) = resolve_sources(&config);
    let target = SmbTarget::parse(&config.smb.url)?;
    print_check_summary(path, &config, &target, &found);
    if missing.is_empty() {
        print_check_ok();
        Ok(Exit::Success)
    } else {
        print_check_missing(&missing);
        Ok(Exit::Failure)
    }
}

/// List the configured sources: success when they all exist, warnings otherwise
fn sources(path: &Path) -> Result<Exit> {
    let config = Config::load(path)?;
    Ok(if print_sources(&config, true) {
        Exit::Success
    } else {
        Exit::Warnings
    })
}

/// Print every configured source (✓ found on disk, ✗ missing) and the exclude
/// patterns (or only their count, so a long exclude list does not hide the
/// sources); returns true when all the sources exist. Shared with the console
/// backup flow, so the user sees what "configured sources" means before choosing.
pub(crate) fn print_sources(config: &Config, list_excludes: bool) -> bool {
    let sources = &config.backup.sources;
    ui::say!("📁 Configured sources ({})", sources.len());
    let mut all_found = true;
    for source in sources {
        match expand_tilde(source).canonicalize() {
            Ok(p) if p.as_os_str() == source.as_str() => ui::say!("    ✓ {source}"),
            Ok(p) => ui::say!("    ✓ {source}  →  {}", p.display()),
            Err(_) => {
                all_found = false;
                ui::say!("    ✗ {source}  (not found: it will be skipped)");
            }
        }
    }
    if sources.is_empty() {
        all_found = false;
        ui::say!("    (none: set backup.sources in the configuration)");
    }
    let excludes = &config.backup.excludes;
    if list_excludes {
        ui::say!("🚫 Exclude patterns ({})", excludes.len());
        for pattern in excludes {
            ui::say!("    • {pattern}");
        }
    } else {
        ui::say!(
            "🚫 {} exclude pattern(s) apply (list: `heelonbackup config sources`)",
            excludes.len()
        );
    }
    all_found
}

/// Resolve the configured backup sources: (existing canonical paths, missing source strings)
fn resolve_sources(config: &Config) -> (Vec<PathBuf>, Vec<String>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for source in &config.backup.sources {
        let path = expand_tilde(source);
        match path.canonicalize() {
            Ok(p) => found.push(p),
            Err(_) => missing.push(source.clone()),
        }
    }
    found.sort();
    (found, missing)
}

/// Read one line from stdin, `default` when empty
// CLI only (`config init`): the console never asks questions on stdin
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
// Every line goes through `ui::say!`: printed as-is by the CLI, shown in the
// terminal pane by the console.

fn print_config_path(path: &Path) {
    ui::say!("{}", path.display());
}

fn print_creating(path: &Path) {
    ui::say!("Creating {}", path.display());
}

fn print_url_error(e: &ConfigError) {
    ui::say!("  {e}");
}

fn print_config_written(path: &Path) {
    ui::say!("✅ Configuration written to {} (mode 600)", path.display());
    ui::say!(
        "   The password is not stored: it is asked at each run, or read from {PASSWORD_ENV}."
    );
    ui::say!("   Next: `heelonbackup backup --dry-run` to check the setup.");
}

/// The configuration file content (password masked), with an optional permissions warning
fn print_config(path: &Path, json: &str, warning: Option<&str>) {
    ui::say!("# {}", path.display());
    ui::say!("{json}");
    if let Some(warning) = warning {
        ui::say!("⚠️  {warning}");
    }
}

fn print_config_valid() {
    ui::say!("✅ Configuration is valid");
}

fn print_config_invalid(e: &ConfigError) {
    ui::say!("❌ {e}");
    if let Some(source) = std::error::Error::source(e) {
        ui::say!("   {source}");
    }
}

/// Summary of a `config check`: file, SMB target, sources, tuning
fn print_check_summary(path: &Path, config: &Config, target: &SmbTarget, found: &[PathBuf]) {
    ui::say!("# {}", path.display());
    ui::say!(
        "  smb.url:       {} (service {})",
        config.smb.url,
        target.service()
    );
    ui::say!("  smb.username:  {}", config.smb.username);
    ui::say!(
        "  encryption:    {}",
        if config.smb.encrypt { "on" } else { "off" }
    );
    ui::say!(
        "  remote base:   {}",
        config.remote_base().unwrap_or_else(|_| "-".into())
    );
    ui::say!(
        "  sources:       {} configured, {} found on disk",
        config.backup.sources.len(),
        found.len()
    );
    for p in found {
        ui::say!("    ✓ {}", p.display());
    }
    ui::say!(
        "  excludes:      {} pattern(s)",
        config.backup.excludes.len()
    );
    ui::say!("  workers:       {}", config.backup.workers);
    ui::say!(
        "  max_file_size: {}",
        if config.backup.max_file_size == 0 {
            "unlimited".to_string()
        } else {
            ui::bytes(config.backup.max_file_size)
        }
    );
    ui::say!(
        "  retention:     {}",
        match config.storage.retention {
            0 => "keep all".to_string(),
            n => format!("keep {n}"),
        }
    );
    ui::say!("  log_level:     {}", config.log_level);
    ui::say!("  language:      {}", config.language);
}

fn print_check_ok() {
    ui::say!("✅ Configuration is valid (no network access was needed).");
}

fn print_check_missing(missing: &[String]) {
    ui::say!("❌ {} configured source(s) do not exist:", missing.len());
    for m in missing {
        ui::say!("    • {m}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_sources_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.backup.sources = vec![
            dir.path().display().to_string(),
            "/nonexistent/heelonbackup-source".to_string(),
        ];
        let (found, missing) = resolve_sources(&config);
        assert_eq!(found, vec![dir.path().canonicalize().unwrap()]);
        assert_eq!(
            missing,
            vec!["/nonexistent/heelonbackup-source".to_string()]
        );
    }

    #[test]
    fn check_ok_on_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut config = Config::default();
        config.backup.sources = vec![dir.path().display().to_string()];
        config.save(&path).unwrap();
        assert_eq!(check(&path).unwrap(), Exit::Success);
    }

    #[test]
    fn check_fails_when_no_source_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut config = Config::default();
        config.backup.sources = vec!["/nonexistent/heelonbackup-source".to_string()];
        config.save(&path).unwrap();
        assert_eq!(check(&path).unwrap(), Exit::Failure);
    }

    #[test]
    fn check_errors_on_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.json");
        assert!(check(&path).is_err());
    }
}
