//! Dialogs asked by the console flows, answered by the dashboard
//!
//! A flow runs as its own task and calls [`select`], [`confirm`], [`input`] or
//! [`password`] like ordinary async functions: each one sends a [`Request`] to
//! the dashboard loop, which shows a modal dialog and sends the answer back.
//! `Ok(None)` = Esc / Ctrl-C: the flow goes back to the menu.

use crate::error::{BackupError, Result};
use std::sync::Mutex;
use tokio::sync::{mpsc, oneshot};

/// What a dialog asks for
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Pick one item in a list
    Select { items: Vec<String>, default: usize },
    /// Free text (`masked`: password, never displayed)
    Input {
        default: Option<String>,
        masked: bool,
    },
}

/// Answer to a dialog
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer {
    Index(usize),
    Text(String),
}

/// Message from a flow to the dashboard loop
pub(crate) enum Request {
    Ask {
        title: String,
        kind: Kind,
        reply: oneshot::Sender<Option<Answer>>,
    },
    /// Leave the full-screen display (to run an external editor); answered once done
    Suspend(oneshot::Sender<()>),
    /// Come back to the full-screen display
    Resume(oneshot::Sender<()>),
}

static CHANNEL: Mutex<Option<mpsc::UnboundedSender<Request>>> = Mutex::new(None);

/// Connect the flows to the dashboard loop (`None`: disconnect)
pub(crate) fn connect(sender: Option<mpsc::UnboundedSender<Request>>) {
    *CHANNEL.lock().unwrap_or_else(|p| p.into_inner()) = sender;
}

fn send(request: Request) -> Result<()> {
    CHANNEL
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .and_then(|tx| tx.send(request).ok())
        .ok_or(BackupError::Interrupted)
}

async fn ask(title: &str, kind: Kind) -> Result<Option<Answer>> {
    let (reply, answer) = oneshot::channel();
    send(Request::Ask {
        title: title.to_string(),
        kind,
        reply,
    })?;
    // dashboard gone (console closing): behave like a cancellation
    Ok(answer.await.unwrap_or(None))
}

/// Select one item in a list
pub(crate) async fn select(title: &str, items: &[&str], default: usize) -> Result<Option<usize>> {
    let kind = Kind::Select {
        items: items.iter().map(|s| s.to_string()).collect(),
        default,
    };
    Ok(match ask(title, kind).await? {
        Some(Answer::Index(i)) => Some(i),
        _ => None,
    })
}

/// Yes/No question, rendered as a two-item select so it follows the active language
pub(crate) async fn confirm(
    title: &str,
    yes: &str,
    no: &str,
    default_yes: bool,
) -> Result<Option<bool>> {
    let index = select(title, &[yes, no], usize::from(!default_yes)).await?;
    Ok(index.map(|i| i == 0))
}

/// Free-text input (empty allowed)
pub(crate) async fn input(title: &str, default: Option<&str>) -> Result<Option<String>> {
    text(title, default, false).await
}

/// Masked input: the typed characters are never displayed
pub(crate) async fn password(title: &str) -> Result<Option<String>> {
    text(title, None, true).await
}

async fn text(title: &str, default: Option<&str>, masked: bool) -> Result<Option<String>> {
    let kind = Kind::Input {
        default: default.map(str::to_string),
        masked,
    };
    Ok(match ask(title, kind).await? {
        Some(Answer::Text(t)) => Some(t),
        _ => None,
    })
}

/// Run `f` outside the full-screen display (e.g. an external editor that needs
/// the real terminal), then come back to the dashboard
pub(crate) async fn suspended<T>(f: impl FnOnce() -> T + Send + 'static) -> Result<T>
where
    T: Send + 'static,
{
    let (done, ack) = oneshot::channel();
    send(Request::Suspend(done))?;
    let _ = ack.await;
    let result = tokio::task::spawn_blocking(f).await;
    let (done, ack) = oneshot::channel();
    send(Request::Resume(done))?;
    let _ = ack.await;
    result.map_err(|e| BackupError::io("external command failed", std::io::Error::other(e)))
}
