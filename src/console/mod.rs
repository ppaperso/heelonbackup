//! Interactive console: full-screen dashboard (ratatui)
//!
//! Opened when `heelonbackup` runs without a subcommand. The header shows the
//! version, environment, date/time and language; panels show the NAS state
//! (TCP check on the SMB port, refreshed in the background) and the last backup;
//! the terminal pane streams the output and progress of the running operation.
//!
//! The console holds no business logic: each flow collects the user's choices
//! through modal dialogs (`prompt.rs`), then calls the same `service::`
//! orchestration as the CLI, whose output is routed here by `ui::install_sink`.
//! Esc / q / Ctrl-C always leave cleanly; while an operation runs, Ctrl-C
//! interrupts it (`service::request_interrupt`) and the dashboard stays open.

mod app;
mod backup;
mod config;
mod prompt;
mod restore;
mod seal;
mod status;
mod strings;
mod validate;
mod verify;
mod view;

pub(crate) use strings::{Lang, Strings};

use self::app::{Action, App, Command, Dialog, Nas};
use self::prompt::Request;
use crate::config::{self as cfg, Config, SmbTarget};
use crate::error::{BackupError, Result};
use crate::service::{self, Exit};
use crate::storage::LocalHistory;
use crate::storage::smb_client::Reachability;
use crate::ui::{self, Stream};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::crossterm::{cursor, execute};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

pub(crate) use prompt::{confirm, input, select};

/// Redraw period (clock, spinners, progress)
const TICK: Duration = Duration::from_millis(200);
/// NAS reachability refresh period
const PROBE_EVERY: Duration = Duration::from_secs(15);

/// Ask the SMB password once per console session (masked dialog, memory only).
/// `Ok(false)`: cancelled by the user.
pub(crate) async fn ensure_password(s: &Strings, config: &Config) -> Result<bool> {
    if cfg::has_session_password() {
        return Ok(true);
    }
    let target = SmbTarget::parse(&config.smb.url)?;
    let title = format!(
        "{} {}@{}",
        s.password_prompt, config.smb.username, target.host
    );
    loop {
        match prompt::password(&title).await? {
            None => return Ok(false),
            Some(p) if p.is_empty() => continue,
            Some(p) => {
                cfg::set_session_password(Some(p));
                return Ok(true);
            }
        }
    }
}

/// Main entry point of the interactive console
pub(crate) async fn run(config_path: &Path) -> Result<Exit> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!(
            "The interactive console needs a terminal; pass a subcommand instead (see `heelonbackup --help`)."
        );
        return Ok(Exit::Failure);
    }
    let lang = Config::load(config_path)
        .map(|c| Lang::from_config(&c.language))
        .unwrap_or(Lang::En);

    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (req_tx, req_rx) = mpsc::unbounded_channel();
    ui::install_sink(out_tx);
    prompt::connect(Some(req_tx));

    let mut terminal = ratatui::init();
    let result = Dashboard::new(config_path.to_path_buf(), lang)
        .run(&mut terminal, out_rx, req_rx)
        .await;
    ratatui::restore();

    prompt::connect(None);
    ui::remove_sink();
    cfg::set_session_password(None);
    result
}

/// Terminal events read on a dedicated thread (crossterm's reader is blocking)
struct EventReader {
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl EventReader {
    fn spawn(tx: mpsc::UnboundedSender<Event>) -> Self {
        let paused = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, st) = (paused.clone(), stop.clone());
        std::thread::spawn(move || {
            while !st.load(Ordering::Relaxed) {
                if p.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                match event::poll(Duration::from_millis(100)) {
                    Ok(true) => match event::read() {
                        Ok(ev) => {
                            if tx.send(ev).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
        });
        Self { paused, stop }
    }
}

impl Drop for EventReader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Dashboard {
    app: App,
    config_path: PathBuf,
    probe_now: Arc<Notify>,
}

impl Dashboard {
    fn new(config_path: PathBuf, lang: Lang) -> Self {
        Self {
            app: App::new(lang),
            config_path,
            probe_now: Arc::new(Notify::new()),
        }
    }

    async fn run(
        mut self,
        terminal: &mut DefaultTerminal,
        mut out_rx: mpsc::UnboundedReceiver<(Stream, String)>,
        mut req_rx: mpsc::UnboundedReceiver<Request>,
    ) -> Result<Exit> {
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel();
        let reader = EventReader::spawn(ev_tx);
        let (nas_tx, mut nas_rx) = mpsc::unbounded_channel();
        let prober = tokio::spawn(probe_loop(
            self.config_path.clone(),
            nas_tx,
            self.probe_now.clone(),
        ));
        // Ctrl-C arrives as a key in raw mode; a SIGINT sent by `kill` is handled the same way
        let mut sigint = signal(SignalKind::interrupt())
            .map_err(|e| BackupError::io("cannot watch SIGINT", e))?;
        let mut tick = tokio::time::interval(TICK);
        let mut flow: Option<JoinHandle<Result<Exit>>> = None;

        self.reload();
        if let Some(warning) = self
            .app
            .config
            .as_ref()
            .and_then(|c| Config::permission_warning(&self.config_path, c))
        {
            self.app.push_line(Stream::Err, format!("⚠ {warning}"));
        }

        let result = loop {
            if !self.app.suspended {
                terminal
                    .draw(|frame| view::draw(frame, &self.app))
                    .map_err(|e| BackupError::io("cannot draw the console", e))?;
            }
            let command = tokio::select! {
                Some(ev) = ev_rx.recv() => match ev {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.app.on_key(key),
                    _ => Command::None,
                },
                Some((stream, line)) = out_rx.recv() => {
                    self.app.push_line(stream, line);
                    while let Ok((stream, line)) = out_rx.try_recv() {
                        self.app.push_line(stream, line);
                    }
                    Command::None
                }
                Some(request) = req_rx.recv() => {
                    self.on_request(request, terminal, &reader).await;
                    Command::None
                }
                Some(nas) = nas_rx.recv() => {
                    self.app.nas = nas;
                    Command::None
                }
                done = async { flow.as_mut().expect("guarded by is_some").await }, if flow.is_some() => {
                    flow = None;
                    // the flow's last lines may still be queued: show them before its result
                    while let Ok((stream, line)) = out_rx.try_recv() {
                        self.app.push_line(stream, line);
                    }
                    self.on_flow_done(done);
                    Command::None
                }
                _ = sigint.recv() => if self.app.running.is_some() {
                    Command::Interrupt
                } else {
                    Command::Quit
                },
                _ = tick.tick() => {
                    self.app.tick = self.app.tick.wrapping_add(1);
                    Command::None
                }
            };
            match command {
                Command::None => {}
                Command::Start(action) => flow = Some(self.start(action)),
                Command::ToggleLanguage => self.toggle_language(),
                Command::Interrupt => {
                    service::request_interrupt();
                    let s = self.app.strings();
                    self.app
                        .push_line(Stream::Err, format!("⏹ {}", s.interrupt_requested));
                }
                Command::Quit => break Ok(Exit::Success),
            }
        };

        self.app.cancel_dialog();
        if let Some(flow) = flow {
            service::request_interrupt();
            flow.abort();
        }
        prober.abort();
        drop(reader);
        result
    }

    /// Reload what the panels show: configuration and local history
    fn reload(&mut self) {
        self.app.config = Config::load(&self.config_path).ok();
        self.app.history = LocalHistory::open().load_all();
    }

    fn start(&mut self, action: Action) -> JoinHandle<Result<Exit>> {
        let s = self.app.strings();
        self.app.running = Some(action);
        self.app.mark_flow_start();
        self.app.info(format!("━━ {} ━━", action.label(s)));
        service::clear_interrupt();
        let path = self.config_path.clone();
        tokio::spawn(async move {
            match action {
                Action::Backup => backup::run(s, &path).await,
                Action::Status => status::run(s, &path).await,
                Action::List => status::run_list(s, &path).await,
                Action::Verify => verify::run(s, &path).await,
                Action::Restore => restore::run(s, &path).await,
                Action::Seal => seal::run(s, &path).await,
                Action::Config => config::run(s, &path).await,
                Action::Language | Action::Quit => Ok(Exit::Success),
            }
        })
    }

    fn on_flow_done(&mut self, done: std::result::Result<Result<Exit>, tokio::task::JoinError>) {
        self.app.running = None;
        self.app.cancel_dialog();
        let s = self.app.strings();
        let result = done.unwrap_or_else(|e| {
            Err(BackupError::io(
                "the operation stopped unexpectedly",
                std::io::Error::other(e),
            ))
        });
        match result {
            Ok(Exit::Success) => self.app.info(format!("✔ {}", s.result_ok)),
            Ok(Exit::Warnings) => self
                .app
                .push_line(Stream::Err, format!("⚠ {}", s.result_warnings)),
            Ok(Exit::Failure) => self
                .app
                .push_line(Stream::Err, format!("✖ {}", s.result_failed)),
            Ok(Exit::Interrupted) | Err(BackupError::Interrupted) => {
                self.app
                    .push_line(Stream::Err, format!("⏹ {}", s.result_interrupted));
            }
            Err(e) => {
                if matches!(e, BackupError::SmbAuth(_)) {
                    cfg::set_session_password(None);
                }
                self.app
                    .push_line(Stream::Err, format!("❌ {}: {e}", s.error));
                let mut source = std::error::Error::source(&e);
                while let Some(cause) = source {
                    self.app
                        .push_line(Stream::Err, format!("   caused by: {cause}"));
                    source = cause.source();
                }
                if matches!(e, BackupError::SmbAuth(_)) {
                    self.app
                        .push_line(Stream::Err, format!("⚠ {}", s.password_rejected));
                }
            }
        }
        self.app.focus_flow_output();
        self.reload();
        self.probe_now.notify_one();
    }

    fn toggle_language(&mut self) {
        self.app.lang = self.app.lang.toggle();
        let s = self.app.strings();
        match persist_language(&self.config_path, self.app.lang) {
            Ok(()) => self.app.info(s.language_saved),
            Err(e) => self
                .app
                .push_line(Stream::Err, format!("❌ {}: {e}", s.error)),
        }
    }

    async fn on_request(
        &mut self,
        request: Request,
        terminal: &mut DefaultTerminal,
        reader: &EventReader,
    ) {
        match request {
            Request::Ask { title, kind, reply } => {
                self.app.open_dialog(Dialog::new(title, kind, reply));
            }
            Request::Suspend(done) => {
                reader.paused.store(true, Ordering::Relaxed);
                // let an in-flight poll of the reader thread finish
                tokio::time::sleep(Duration::from_millis(150)).await;
                let _ = disable_raw_mode();
                let _ = execute!(std::io::stdout(), LeaveAlternateScreen, cursor::Show);
                self.app.suspended = true;
                let _ = done.send(());
            }
            Request::Resume(done) => {
                let _ = enable_raw_mode();
                let _ = execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide);
                let _ = terminal.clear();
                self.app.suspended = false;
                reader.paused.store(false, Ordering::Relaxed);
                let _ = done.send(());
            }
        }
    }
}

/// Check the NAS now, then every `PROBE_EVERY` (or when asked, after an operation)
async fn probe_loop(path: PathBuf, tx: mpsc::UnboundedSender<Nas>, now: Arc<Notify>) {
    loop {
        let state = match Config::load(&path) {
            Ok(config) => match service::remote::reachability(&config).await {
                Ok((target, Reachability::Online(latency))) => Nas::Online {
                    address: service::remote::probed_address(&target),
                    latency,
                },
                Ok((target, Reachability::Offline(reason))) => Nas::Offline {
                    address: service::remote::probed_address(&target),
                    reason,
                },
                Err(_) => Nas::NoConfig,
            },
            Err(_) => Nas::NoConfig,
        };
        if tx.send(state).is_err() {
            return;
        }
        tokio::select! {
            () = tokio::time::sleep(PROBE_EVERY) => {}
            () = now.notified() => {}
        }
    }
}

/// Save the console language in the configuration file
fn persist_language(path: &Path, lang: Lang) -> Result<()> {
    let mut config = Config::load(path).unwrap_or_default();
    config.language = lang.as_str().to_string();
    config.validate()?;
    config.save(path)?;
    Ok(())
}
