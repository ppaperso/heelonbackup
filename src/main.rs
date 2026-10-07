//! HeelonBackup - reliable backup tool for Linux to Synology NAS via SMB

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // `heelonbackup list | head`: exit quietly when stdout is closed instead of panicking.
    // (SIGPIPE stays ignored because smbclient's stdin is a pipe too.)
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        if message.contains("failed printing to stdout") && message.contains("Broken pipe") {
            std::process::exit(141);
        }
        default_hook(info);
    }));

    heelonbackup::cli::run().await
}
