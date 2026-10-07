//! Parallel file transfers with byte-level progress
//!
//! Files are grouped in batches; each batch is transferred by one smbclient session.
//! `workers` sessions run concurrently. smbclient confirms every completed file on stderr,
//! which drives the progress bar and decides which files were really transferred.

use crate::backup::hash_file;
use crate::storage::smb_client::{SmbClient, quote, smb_display_path};
use indicatif::ProgressBar;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::task::JoinSet;
use tracing::{debug, warn};

const BATCH_FILES: usize = 200;
const BATCH_BYTES: u64 = 256 * 1024 * 1024;
/// Consecutive failed sessions after which the transfer is aborted (NAS gone, network down...)
const MAX_SESSION_ERRORS: u32 = 3;

#[derive(Debug, Clone)]
pub struct Item {
    pub local: String,
    /// Path relative to the share root
    pub remote: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `sha256` and `size` of the local file (read before upload, or after download)
    Done {
        sha256: String,
        size: u64,
    },
    Failed {
        reason: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Upload,
    /// Download, then hash the local file; `discard` deletes it afterwards (deep verify)
    Download {
        discard: bool,
    },
}

struct Shared {
    items: Vec<Item>,
    outcomes: Mutex<Vec<Option<Outcome>>>,
    fatal: Mutex<Option<String>>,
    done_files: AtomicU64,
    session_errors: AtomicU32,
    pb: ProgressBar,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A set of files to transfer. Results are kept even if the transfer future is dropped
/// (Ctrl-C), so a partial manifest can still be written.
pub struct Transfer {
    shared: Arc<Shared>,
}

impl Transfer {
    pub fn new(items: Vec<Item>, pb: ProgressBar) -> Self {
        let outcomes = Mutex::new(vec![None; items.len()]);
        Self {
            shared: Arc::new(Shared {
                items,
                outcomes,
                fatal: Mutex::new(None),
                done_files: AtomicU64::new(0),
                session_errors: AtomicU32::new(0),
                pb,
            }),
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.shared.items
    }

    /// Snapshot of the results; `None` = not transferred (interrupted or aborted)
    pub fn outcomes(&self) -> Vec<Option<Outcome>> {
        lock(&self.shared.outcomes).clone()
    }

    /// Reason why the transfer was aborted early, if it was
    pub fn fatal_error(&self) -> Option<String> {
        lock(&self.shared.fatal).clone()
    }

    pub async fn run(&self, client: &SmbClient, mode: Mode, workers: usize) {
        self.shared.update_message();
        let all: Vec<usize> = (0..self.shared.items.len()).collect();
        self.pass(client, mode, workers, all, false).await;

        let retry: Vec<usize> = lock(&self.shared.outcomes)
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                matches!(
                    o,
                    Some(Outcome::Failed {
                        retryable: true,
                        ..
                    })
                )
            })
            .map(|(i, _)| i)
            .collect();
        if !retry.is_empty() && self.fatal_error().is_none() {
            debug!("Retrying {} file(s)", retry.len());
            self.pass(client, mode, workers, retry, true).await;
        }
    }

    async fn pass(
        &self,
        client: &SmbClient,
        mode: Mode,
        workers: usize,
        indices: Vec<usize>,
        last: bool,
    ) {
        let queue = Arc::new(Mutex::new(make_batches(&self.shared.items, indices)));
        let workers = workers.clamp(1, lock(&queue).len().max(1));
        let mut set = JoinSet::new();
        for _ in 0..workers {
            let shared = Arc::clone(&self.shared);
            let queue = Arc::clone(&queue);
            let client = client.clone();
            set.spawn(async move {
                loop {
                    if lock(&shared.fatal).is_some() {
                        break;
                    }
                    let Some(batch) = lock(&queue).pop_front() else {
                        break;
                    };
                    match mode {
                        Mode::Upload => upload_batch(&shared, &client, batch, last).await,
                        Mode::Download { discard } => {
                            download_batch(&shared, &client, batch, last, discard).await
                        }
                    }
                }
            });
        }
        while let Some(res) = set.join_next().await {
            if let Err(e) = res
                && e.is_panic()
            {
                std::panic::resume_unwind(e.into_panic());
            }
        }
    }
}

fn make_batches(items: &[Item], indices: Vec<usize>) -> VecDeque<Vec<usize>> {
    let mut batches = VecDeque::new();
    let mut current = Vec::new();
    let mut bytes = 0;
    for i in indices {
        if !current.is_empty()
            && (current.len() >= BATCH_FILES || bytes + items[i].size > BATCH_BYTES)
        {
            batches.push_back(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes += items[i].size;
        current.push(i);
    }
    if !current.is_empty() {
        batches.push_back(current);
    }
    batches
}

impl Shared {
    fn update_message(&self) {
        self.pb.set_message(format!(
            "{}/{} files",
            self.done_files.load(Ordering::Relaxed),
            self.items.len()
        ));
    }

    /// Count a file as processed in the progress bar
    fn advance(&self, i: usize) {
        self.pb.inc(self.items[i].size);
        self.done_files.fetch_add(1, Ordering::Relaxed);
        self.update_message();
    }

    fn record(&self, i: usize, outcome: Outcome) {
        lock(&self.outcomes)[i] = Some(outcome);
    }

    /// Record a failure; it only counts as processed if it will not be retried
    fn fail(&self, i: usize, reason: String, retryable: bool, last: bool) {
        debug!("{}: {reason}", self.items[i].local);
        if last || !retryable {
            self.advance(i);
        }
        self.record(i, Outcome::Failed { reason, retryable });
    }

    fn session_result(&self, ok: bool, error: Option<&str>) {
        if ok {
            self.session_errors.store(0, Ordering::Relaxed);
            return;
        }
        let count = self.session_errors.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= MAX_SESSION_ERRORS {
            let mut fatal = lock(&self.fatal);
            if fatal.is_none() {
                let reason = error.unwrap_or("repeated SMB session failures").to_string();
                warn!("Aborting transfer: {reason}");
                *fatal = Some(reason);
            }
        }
    }
}

/// Strip ` (X kB/s) (average Y kB/s)` from a smbclient transfer line
fn strip_rates(line: &str) -> &str {
    let mut s = line;
    for _ in 0..2 {
        if let Some(idx) = s.rfind(" (") {
            s = &s[..idx];
        }
    }
    s
}

/// Find the batch item confirmed by `putting file <L> as <\R> (...) (...)` (keyed by remote)
/// or `getting file <\R> of size N as <L> (...) (...)` (keyed by local path)
fn confirmed(line: &str, prefix: &str, pending: &HashMap<String, usize>) -> Option<usize> {
    let rest = strip_rates(line.strip_prefix(prefix)?);
    rest.match_indices(" as ")
        .find_map(|(idx, _)| pending.get(&rest[idx + 4..]).copied())
}

/// Error line mentioning one of the paths
fn reason_for<'a>(errors: &'a [&'a str], paths: [&str; 2]) -> Option<&'a str> {
    errors
        .iter()
        .find(|l| paths.iter().any(|p| l.contains(p)))
        .copied()
}

async fn upload_batch(shared: &Shared, client: &SmbClient, batch: Vec<usize>, last: bool) {
    // Hash first: the checksum describes exactly what was read for the upload
    let paths: Vec<String> = batch
        .iter()
        .map(|&i| shared.items[i].local.clone())
        .collect();
    let hashes = tokio::task::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| hash_file(Path::new(p)))
            .collect::<Vec<_>>()
    })
    .await
    .expect("hashing task panicked");

    let mut script = String::new();
    let mut pending = HashMap::new();
    let mut hashed = HashMap::new();
    for (&i, hash) in batch.iter().zip(hashes) {
        let item = &shared.items[i];
        match hash {
            Err(e) => shared.fail(i, format!("cannot read: {e}"), false, last),
            Ok(h) => match (quote(&item.local), quote(&item.remote)) {
                (Ok(l), Ok(r)) => {
                    script.push_str(&format!("put {l} {r}\n"));
                    pending.insert(smb_display_path(&item.remote), i);
                    hashed.insert(i, h);
                }
                (Err(e), _) | (_, Err(e)) => shared.fail(i, e.to_string(), false, last),
            },
        }
    }
    if script.is_empty() {
        return;
    }

    let result = client
        .run_script_with(&script, |line| {
            if let Some(i) = confirmed(line, "putting file ", &pending)
                && let Some((sha256, size)) = hashed.remove(&i)
            {
                shared.record(i, Outcome::Done { sha256, size });
                shared.advance(i);
            }
        })
        .await;

    finish_batch(shared, result, hashed.into_keys(), last);
}

async fn download_batch(
    shared: &Shared,
    client: &SmbClient,
    batch: Vec<usize>,
    last: bool,
    discard: bool,
) {
    let mut script = String::new();
    let mut pending = HashMap::new();
    for &i in &batch {
        let item = &shared.items[i];
        match (quote(&item.remote), quote(&item.local)) {
            (Ok(r), Ok(l)) => {
                script.push_str(&format!("get {r} {l}\n"));
                pending.insert(item.local.clone(), i);
            }
            (Err(e), _) | (_, Err(e)) => shared.fail(i, e.to_string(), false, last),
        }
    }
    if script.is_empty() {
        return;
    }

    let mut received = Vec::new();
    let result = client
        .run_script_with(&script, |line| {
            if let Some(i) = confirmed(line, "getting file ", &pending)
                && pending.remove(&shared.items[i].local).is_some()
            {
                shared.advance(i);
                received.push(i);
            }
        })
        .await;

    let paths: Vec<String> = received
        .iter()
        .map(|&i| shared.items[i].local.clone())
        .collect();
    let hashes = tokio::task::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| {
                let res = hash_file(Path::new(p));
                if discard {
                    let _ = std::fs::remove_file(p);
                }
                res
            })
            .collect::<Vec<_>>()
    })
    .await
    .expect("hashing task panicked");
    for (i, hash) in received.into_iter().zip(hashes) {
        shared.record(
            i,
            match hash {
                Ok((sha256, size)) => Outcome::Done { sha256, size },
                Err(e) => Outcome::Failed {
                    reason: format!("cannot read downloaded file: {e}"),
                    retryable: false,
                },
            },
        );
    }

    finish_batch(shared, result, pending.into_values(), last);
}

/// Mark the files that smbclient did not confirm as failed, with the best explanation found
fn finish_batch(
    shared: &Shared,
    result: crate::error::Result<crate::storage::smb_client::ScriptOutput>,
    unconfirmed: impl Iterator<Item = usize>,
    last: bool,
) {
    let unconfirmed: Vec<usize> = unconfirmed.collect();
    match result {
        Ok(out) => {
            shared.session_result(true, None);
            if unconfirmed.is_empty() {
                return;
            }
            let errors: Vec<&str> = out.errors().collect();
            for i in unconfirmed {
                let item = &shared.items[i];
                let display = smb_display_path(&item.remote);
                let reason = reason_for(&errors, [&display, &item.local])
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        if out.success {
                            "not confirmed by smbclient".to_string()
                        } else {
                            "SMB session ended unexpectedly".to_string()
                        }
                    });
                shared.fail(i, reason, true, last);
            }
        }
        Err(e) => {
            let reason = e.to_string();
            shared.session_result(false, Some(&reason));
            for i in unconfirmed {
                shared.fail(i, reason.clone(), true, last);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_confirmation_lines() {
        let mut pending = HashMap::new();
        pending.insert("\\b\\x as y (1).txt".to_string(), 7);
        let line = "putting file /home/u/x as y (1).txt as \\b\\x as y (1).txt (30000.0 kB/s) (average inf kB/s)";
        assert_eq!(confirmed(line, "putting file ", &pending), Some(7));
        assert_eq!(
            confirmed(
                "putting file /a as \\c (1 kB/s) (average 1 kB/s)",
                "putting file ",
                &pending
            ),
            None
        );

        let mut pending = HashMap::new();
        pending.insert("/tmp/r/g as h.txt".to_string(), 3);
        let line = "getting file \\t1\\s s\\f.txt of size 3 as /tmp/r/g as h.txt (30000.0 KiloBytes/sec) (average inf KiloBytes/sec)";
        assert_eq!(confirmed(line, "getting file ", &pending), Some(3));
    }

    #[test]
    fn batches_respect_limits() {
        let item = |size| Item {
            local: String::new(),
            remote: String::new(),
            size,
        };
        let mut items: Vec<Item> = (0..450).map(|_| item(1)).collect();
        items.push(item(BATCH_BYTES * 2));
        items.push(item(1));
        let batches = make_batches(&items, (0..items.len()).collect());
        let sizes: Vec<usize> = batches.iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![200, 200, 50, 1, 1]);
    }
}
