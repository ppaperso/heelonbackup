//! Terminal output helpers: progress bars, human-readable sizes and durations

use crate::config::Environment;
use chrono::{DateTime, Local};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use std::io::{self, IsTerminal, Write};
use std::sync::LazyLock;
use std::time::Duration;

/// All progress bars are drawn through this so that log lines never corrupt them
pub static PROGRESS: LazyLock<MultiProgress> = LazyLock::new(MultiProgress::new);

/// `io::Write` for tracing that suspends progress bars while a log line is written
pub struct LogWriter;

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        PROGRESS.suspend(|| io::stderr().write_all(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

pub fn bytes(n: u64) -> String {
    HumanBytes(n).to_string()
}

/// `1h 02m 03s`, `4m 05s`, `12s`
pub fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else if secs > 0 {
        format!("{s}s")
    } else {
        format!("{}ms", d.as_millis())
    }
}

pub fn rate(bytes_per_sec: f64) -> String {
    format!("{}/s", bytes(bytes_per_sec as u64))
}

/// Human estimate for transferring `total` bytes at the given rate
pub fn estimate(total: u64, rate_bps: Option<f64>) -> String {
    match rate_bps {
        _ if total == 0 => "nothing to transfer".to_string(),
        Some(r) if r > 0.0 => format!(
            "~{} (at {}, measured during the last backup)",
            duration(Duration::from_secs_f64(total as f64 / r)),
            rate(r)
        ),
        _ => "unknown until the first backup has completed".to_string(),
    }
}

/// Spinner used while scanning or preparing
pub fn spinner(message: impl Into<String>) -> ProgressBar {
    let pb = PROGRESS.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg} [{elapsed}]").expect("valid template"),
    );
    pb.set_message(message.into());
    pb.enable_steady_tick(Duration::from_millis(120));
    pb
}

/// Byte-based transfer bar with throughput and ETA; the message shows the file counter
pub fn transfer_bar(total_bytes: u64, prefix: &str) -> ProgressBar {
    let pb = PROGRESS.add(ProgressBar::new(total_bytes));
    pb.set_style(
        ProgressStyle::with_template(
            "{prefix:.bold} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} \
             {binary_bytes_per_sec} ETA {eta} | {msg}",
        )
        .expect("valid template")
        .progress_chars("=> "),
    );
    pb.set_prefix(prefix.to_string());
    pb.enable_steady_tick(Duration::from_millis(250));
    pb
}

pub const RULE: &str = "────────────────────────────────────────────────────────────";

/// `3 days ago`, `5 min ago`
pub fn age(date: DateTime<Local>) -> String {
    let secs = (Local::now() - date).num_seconds().max(0);
    match secs {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}

/// `2026-01-07 14:32:05`
pub fn date(d: DateTime<Local>) -> String {
    d.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Print at most `max` items, then `... and N more`
pub fn print_limited<T>(items: &[T], max: usize, show: impl Fn(&T) -> String) {
    for item in items.iter().take(max) {
        println!("    • {}", show(item));
    }
    if items.len() > max {
        println!("    … and {} more", items.len() - max);
    }
}

/// Warn on stderr when running in the development environment (silent in production)
pub fn print_environment_banner() {
    if Environment::detect() != Environment::Dev {
        return;
    }
    let label = "DEV MODE - test configuration and dev_* folder on the NAS";
    if io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        eprintln!("\x1b[1;31m⚠ {label}\x1b[0m");
    } else {
        eprintln!("[{label}]");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(duration(Duration::from_millis(250)), "250ms");
        assert_eq!(duration(Duration::from_secs(12)), "12s");
        assert_eq!(duration(Duration::from_secs(245)), "4m 05s");
        assert_eq!(duration(Duration::from_secs(3723)), "1h 02m 03s");
    }

    #[test]
    fn estimates() {
        assert!(estimate(1_000, None).starts_with("unknown"));
        assert!(estimate(600 * 1024 * 1024, Some(1024.0 * 1024.0)).starts_with("~10m 00s"));
    }
}
