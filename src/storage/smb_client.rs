//! SMB access through the system `smbclient` (Samba) in batch mode
//!
//! Each [`SmbClient::run_script`] call opens one SMB session and executes a list of
//! smbclient commands read from stdin. The password is passed through the `PASSWD`
//! environment variable of the child process, never on the command line or on disk.

use crate::config::{Config, SmbTarget, is_safe_for_smbclient};
use crate::error::{BackupError, Result};
use std::collections::BTreeSet;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tracing::debug;

const SMBCLIENT: &str = "smbclient";

/// Output of a batch session
#[derive(Debug, Default)]
pub struct ScriptOutput {
    /// smbclient prints most errors on stdout in batch mode
    pub stdout: String,
    /// ... and transfer confirmations on stderr
    pub stderr: String,
    pub success: bool,
}

impl ScriptOutput {
    /// Lines reporting an NT_STATUS error
    pub fn errors(&self) -> impl Iterator<Item = &str> {
        self.stdout
            .lines()
            .chain(self.stderr.lines())
            .filter(|l| l.contains("NT_STATUS_") || l.ends_with(" does not exist"))
    }
}

/// Entry returned by [`SmbClient::list`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// Path relative to the share root, `/`-separated
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Clone)]
pub struct SmbClient {
    target: SmbTarget,
    username: String,
    password: String,
    workgroup: Option<String>,
    timeout: u64,
    encrypt: bool,
    /// smbclient < 4.15 only knows `-e`
    legacy_encrypt_flag: bool,
}

impl std::fmt::Debug for SmbClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmbClient")
            .field("target", &self.target.to_string())
            .field("username", &self.username)
            .field("encrypt", &self.encrypt)
            .finish_non_exhaustive()
    }
}

/// Quote a remote or local path for an smbclient command
pub fn quote(path: &str) -> Result<String> {
    if is_safe_for_smbclient(path) {
        Ok(format!("\"{path}\""))
    } else {
        Err(BackupError::InvalidPath(format!(
            "{path:?} contains a quote, backslash or control character"
        )))
    }
}

/// `\a\b c` as printed by smbclient for the remote path `a/b c`
pub fn smb_display_path(remote: &str) -> String {
    format!("\\{}", remote.trim_start_matches('/').replace('/', "\\"))
}

impl SmbClient {
    /// Check that smbclient is available and that the share is reachable with these credentials
    pub async fn connect(config: &Config, password: String) -> Result<Self> {
        let target = SmbTarget::parse(&config.smb.url)?;
        let version = smbclient_version().await?;
        let client = Self {
            target,
            username: config.smb.username.clone(),
            password,
            workgroup: config.smb.workgroup.clone().filter(|w| !w.is_empty()),
            timeout: config.smb.timeout,
            encrypt: config.smb.encrypt,
            legacy_encrypt_flag: version < (4, 15),
        };
        let out = client.run_script("ls \"*\"\n").await?;
        if !out.success {
            let detail = out
                .errors()
                .next()
                .map(str::to_string)
                .unwrap_or_else(|| last_line(&out));
            let hint = if client.encrypt {
                " (if the NAS does not support SMB3 encryption, set smb.encrypt to false)"
            } else {
                ""
            };
            return Err(BackupError::SmbConnection(format!("{detail}{hint}")));
        }
        Ok(client)
    }

    pub fn target(&self) -> &SmbTarget {
        &self.target
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(SMBCLIENT);
        cmd.arg(self.target.service())
            .arg("-U")
            .arg(&self.username)
            .arg("-t")
            .arg(self.timeout.to_string())
            .env("PASSWD", &self.password)
            .env("LC_ALL", "C")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(port) = self.target.port {
            cmd.arg("-p").arg(port.to_string());
        }
        if let Some(workgroup) = &self.workgroup {
            cmd.arg("-W").arg(workgroup);
        }
        if self.encrypt {
            if self.legacy_encrypt_flag {
                cmd.arg("-e");
            } else {
                cmd.arg("--client-protection=encrypt");
            }
        }
        cmd
    }

    pub async fn run_script(&self, script: &str) -> Result<ScriptOutput> {
        self.run_script_with(script, |_| {}).await
    }

    /// Run a batch session; `on_stderr_line` receives each stderr line as soon as it is printed
    /// (smbclient reports completed transfers there).
    pub async fn run_script_with(
        &self,
        script: &str,
        mut on_stderr_line: impl FnMut(&str),
    ) -> Result<ScriptOutput> {
        let mut child = self.command().spawn().map_err(|e| {
            BackupError::io(
                "cannot run smbclient (install the `samba-client` package)",
                e,
            )
        })?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let write = async {
            // A broken pipe means smbclient exited early; its output explains why
            let _ = stdin.write_all(script.as_bytes()).await;
            let _ = stdin.write_all(b"exit\n").await;
            drop(stdin);
        };
        let read_out = async {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).into_owned()
        };
        let read_err = async {
            let mut reader = BufReader::new(stderr);
            let mut all = String::new();
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let text = String::from_utf8_lossy(&line);
                        on_stderr_line(text.trim_end_matches(['\n', '\r']));
                        all.push_str(&text);
                    }
                }
            }
            all
        };
        let ((), stdout, stderr) = tokio::join!(write, read_out, read_err);
        let status = child
            .wait()
            .await
            .map_err(|e| BackupError::io("smbclient did not terminate correctly", e))?;

        let out = ScriptOutput {
            stdout,
            stderr,
            success: status.success(),
        };
        debug!(
            "smbclient exited with {status} ({} script bytes)",
            script.len()
        );
        session_error(&out)?;
        Ok(out)
    }

    /// Create directories (and all their parents). Existing directories are fine.
    pub async fn mkdirs<'a>(&self, dirs: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let mut all = BTreeSet::new();
        for dir in dirs {
            let dir = dir.trim_matches('/');
            for (i, _) in dir.match_indices('/') {
                all.insert(&dir[..i]);
            }
            if !dir.is_empty() {
                all.insert(dir);
            }
        }
        if all.is_empty() {
            return Ok(());
        }
        // BTreeSet order guarantees that parents come before their children
        let mut script = String::new();
        for dir in &all {
            script.push_str(&format!("mkdir {}\n", quote(dir)?));
        }
        let out = self.run_script(&script).await?;
        let errors: Vec<&str> = out
            .errors()
            .filter(|l| !l.contains("NT_STATUS_OBJECT_NAME_COLLISION"))
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(BackupError::SmbProtocol(format!(
                "cannot create remote directories: {}",
                errors
                    .iter()
                    .take(3)
                    .copied()
                    .collect::<Vec<_>>()
                    .join("; ")
            )))
        }
    }

    /// Upload a small in-memory file (manifests)
    pub async fn upload_bytes(&self, remote: &str, content: &[u8]) -> Result<()> {
        let tmp = tempfile::NamedTempFile::new()
            .map_err(|e| BackupError::io("cannot create temporary file", e))?;
        std::fs::write(tmp.path(), content)
            .map_err(|e| BackupError::io("cannot write temporary file", e))?;
        let local = tmp
            .path()
            .to_str()
            .ok_or_else(|| BackupError::InvalidPath(tmp.path().display().to_string()))?;
        let script = format!("put {} {}\n", quote(local)?, quote(remote)?);
        let out = self.run_script(&script).await?;
        if out.stderr.contains("putting file ") {
            Ok(())
        } else {
            Err(BackupError::SmbProtocol(format!(
                "cannot upload {remote}: {}",
                out.errors()
                    .next()
                    .unwrap_or("no confirmation from smbclient")
            )))
        }
    }

    /// Download a small remote file into memory (manifests)
    pub async fn download_bytes(&self, remote: &str) -> Result<Vec<u8>> {
        let dir =
            tempfile::tempdir().map_err(|e| BackupError::io("cannot create temporary dir", e))?;
        let local = dir.path().join("download");
        let local_str = local
            .to_str()
            .ok_or_else(|| BackupError::InvalidPath(local.display().to_string()))?;
        let script = format!("get {} {}\n", quote(remote)?, quote(local_str)?);
        let out = self.run_script(&script).await?;
        if !out.stderr.contains("getting file ") {
            if out.errors().any(is_not_found) {
                return Err(BackupError::BackupNotFound(remote.to_string()));
            }
            return Err(BackupError::SmbProtocol(format!(
                "cannot download {remote}: {}",
                out.errors()
                    .next()
                    .unwrap_or("no confirmation from smbclient")
            )));
        }
        std::fs::read(&local).map_err(|e| BackupError::io("cannot read downloaded file", e))
    }

    /// List a remote directory (relative to the share root). A missing directory is empty.
    pub async fn list(&self, dir: &str, recursive: bool) -> Result<Vec<RemoteEntry>> {
        let dir = dir.trim_matches('/');
        let pattern = if dir.is_empty() {
            "*".to_string()
        } else {
            format!("{dir}/*")
        };
        let script = format!(
            "recurse {}\nls {}\n",
            if recursive { "ON" } else { "OFF" },
            quote(&pattern)?
        );
        let out = self.run_script(&script).await?;
        if let Some(err) = out.errors().find(|l| !is_not_found(l)) {
            return Err(BackupError::SmbProtocol(format!(
                "cannot list {dir}: {err}"
            )));
        }
        Ok(parse_listing(&out.stdout, dir))
    }

    /// Recursively delete a remote directory
    pub async fn deltree(&self, dir: &str) -> Result<()> {
        let out = self
            .run_script(&format!("deltree {}\n", quote(dir)?))
            .await?;
        match out.errors().next() {
            Some(err) => Err(BackupError::SmbProtocol(format!(
                "cannot delete {dir}: {err}"
            ))),
            None => Ok(()),
        }
    }
}

fn is_not_found(line: &str) -> bool {
    line.contains("NT_STATUS_OBJECT_NAME_NOT_FOUND")
        || line.contains("NT_STATUS_OBJECT_PATH_NOT_FOUND")
        || line.contains("NT_STATUS_NO_SUCH_FILE")
}

fn last_line(out: &ScriptOutput) -> String {
    out.stdout
        .lines()
        .chain(out.stderr.lines())
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("smbclient failed without explanation")
        .to_string()
}

/// Errors that prevent the whole session from working
fn session_error(out: &ScriptOutput) -> Result<()> {
    let text = format!("{}\n{}", out.stdout, out.stderr);
    // Only match at the start of a line: file names may contain these words
    let find = |prefix: &str| text.lines().map(str::trim).find(|l| l.starts_with(prefix));
    if let Some(line) = find("session setup failed") {
        return Err(BackupError::SmbAuth(format!(
            "{line} (check smb.username and the password)"
        )));
    }
    if let Some(line) = find("tree connect failed") {
        let hint = if line.contains("BAD_NETWORK_NAME") {
            " (the share does not exist; check smb.url)"
        } else {
            ""
        };
        return Err(BackupError::SmbConnection(format!("{line}{hint}")));
    }
    if let Some(line) = find("do_connect:").or_else(|| find("Connection to ")) {
        return Err(BackupError::SmbConnection(format!(
            "{line} (is the NAS on and reachable on this network?)"
        )));
    }
    Ok(())
}

async fn smbclient_version() -> Result<(u32, u32)> {
    let output = Command::new(SMBCLIENT)
        .arg("--version")
        .env("LC_ALL", "C")
        .output()
        .await
        .map_err(|e| {
            BackupError::io(
                "smbclient not found (install it, e.g. `sudo dnf install samba-client` \
                 or `sudo apt install smbclient`)",
                e,
            )
        })?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_version(&text).unwrap_or((4, 15)))
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let version = text.split_whitespace().nth(1)?;
    let mut parts = version.split(['.', '-']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Parse the output of `recurse ON; ls "dir/*"`
fn parse_listing(stdout: &str, dir: &str) -> Vec<RemoteEntry> {
    let mut current = dir.to_string();
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if let Some(header) = line.strip_prefix('\\') {
            current = header.replace('\\', "/").trim_matches('/').to_string();
            continue;
        }
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        let Some((name, attrs, size)) = parse_entry(rest) else {
            continue;
        };
        if name == "." || name == ".." {
            continue;
        }
        let path = if current.is_empty() {
            name.to_string()
        } else {
            format!("{current}/{name}")
        };
        entries.push(RemoteEntry {
            path,
            size,
            is_dir: attrs.contains('D'),
        });
    }
    entries
}

/// `name<padding> ATTRS SIZE Wed Oct  7 15:17:18 2026`
fn parse_entry(line: &str) -> Option<(&str, &str, u64)> {
    let mut rest = line.trim_end();
    let mut tokens = Vec::with_capacity(7);
    for _ in 0..7 {
        let idx = rest.rfind(char::is_whitespace)?;
        tokens.push(&rest[idx + 1..]);
        rest = rest[..idx].trim_end();
    }
    let size = tokens[5].parse().ok()?;
    let attrs = tokens[6];
    if attrs.is_empty() || !attrs.chars().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let name = rest.trim_end();
    (!name.is_empty()).then_some((name, attrs, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_recursive_listing() {
        let out = "  .                                   D        0  Wed Oct  7 15:26:49 2026
  ..                                  D        0  Wed Oct  7 15:26:49 2026
  s s                                 D        0  Wed Oct  7 15:26:49 2026
  top.txt                             A       12  Wed Oct  7 15:26:49 2026

\\t1\\s s
  .                                   D        0  Wed Oct  7 15:26:49 2026
  f;  x.txt                           AH  123456  Wed Oct  7 15:26:49 2026

\t\t48972676 blocks of size 1024. 48201316 blocks available
";
        let entries = parse_listing(out, "t1");
        assert_eq!(
            entries,
            vec![
                RemoteEntry {
                    path: "t1/s s".into(),
                    size: 0,
                    is_dir: true
                },
                RemoteEntry {
                    path: "t1/top.txt".into(),
                    size: 12,
                    is_dir: false
                },
                RemoteEntry {
                    path: "t1/s s/f;  x.txt".into(),
                    size: 123456,
                    is_dir: false
                },
            ]
        );
    }

    #[test]
    fn detects_session_errors() {
        let auth = ScriptOutput {
            stdout: "session setup failed: NT_STATUS_LOGON_FAILURE\n".into(),
            ..Default::default()
        };
        assert!(matches!(session_error(&auth), Err(BackupError::SmbAuth(_))));
        let conn = ScriptOutput {
            stderr:
                "do_connect: Connection to 10.0.0.9 failed (Error NT_STATUS_CONNECTION_REFUSED)\n"
                    .into(),
            ..Default::default()
        };
        assert!(matches!(
            session_error(&conn),
            Err(BackupError::SmbConnection(_))
        ));
        let ok = ScriptOutput {
            stdout: "NT_STATUS_OBJECT_NAME_COLLISION making remote directory \\a\n".into(),
            ..Default::default()
        };
        assert!(session_error(&ok).is_ok());
    }

    #[test]
    fn quoting_and_versions() {
        assert_eq!(quote("a b;c").unwrap(), "\"a b;c\"");
        assert!(quote("a\"b").is_err());
        assert!(quote("a\nput x y").is_err());
        assert_eq!(smb_display_path("a/b c/d"), "\\a\\b c\\d");
        assert_eq!(parse_version("Version 4.24.7\n"), Some((4, 24)));
        assert_eq!(parse_version("Version 4.13.17-Ubuntu"), Some((4, 13)));
    }
}
