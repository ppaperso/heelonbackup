//! Dashboard state and keyboard handling — pure logic, tested without a terminal
//!
//! The dashboard loop (`mod.rs`) feeds events in; this module decides what
//! they mean (menu move, shortcut, dialog answer, interrupt, ...) and keeps
//! everything the view draws: NAS state, history, terminal lines, dialog.

use super::prompt::{Answer, Kind};
use super::strings::{Lang, Strings};
use crate::config::Config;
use crate::storage::BackupManifest;
use crate::ui::Stream;
use chrono::Local;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::oneshot;

/// Lines kept in the terminal pane
const MAX_LINES: usize = 5000;
/// Lines moved by PgUp / PgDn
const PAGE: usize = 10;

/// Action behind a menu entry or a shortcut key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Backup,
    Status,
    List,
    Verify,
    Restore,
    Config,
    Language,
    Quit,
}

/// Menu entries, in display order
pub(crate) const MENU: [Action; 8] = [
    Action::Backup,
    Action::Status,
    Action::List,
    Action::Verify,
    Action::Restore,
    Action::Config,
    Action::Language,
    Action::Quit,
];

impl Action {
    pub(crate) fn label(self, s: &Strings) -> &'static str {
        match self {
            Self::Backup => s.menu_backup,
            Self::Status => s.menu_status,
            Self::List => s.menu_list,
            Self::Verify => s.menu_verify,
            Self::Restore => s.menu_restore,
            Self::Config => s.menu_config,
            Self::Language => s.menu_language,
            Self::Quit => s.menu_quit,
        }
    }

    /// Shortcut key shown in front of the menu entry
    pub(crate) fn key(self) -> char {
        match self {
            Self::Backup => 'b',
            Self::Status => 's',
            Self::List => 'n',
            Self::Verify => 'v',
            Self::Restore => 'r',
            Self::Config => 'c',
            Self::Language => 'L',
            Self::Quit => 'q',
        }
    }

    fn from_key(c: char) -> Option<Self> {
        MENU.into_iter()
            .find(|a| a.key() == c || (*a == Self::Language && c == 'l'))
    }

    /// Actions that run a flow (as opposed to Language / Quit)
    pub(crate) fn is_flow(self) -> bool {
        !matches!(self, Self::Language | Self::Quit)
    }
}

/// NAS reachability as shown in the dashboard
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Nas {
    Checking,
    Online { address: String, latency: Duration },
    Offline { address: String, reason: String },
    NoConfig,
}

/// A modal dialog asked by the running flow
pub(crate) struct Dialog {
    pub title: String,
    pub kind: Kind,
    pub selected: usize,
    pub text: String,
    reply: Option<oneshot::Sender<Option<Answer>>>,
}

impl Dialog {
    pub(crate) fn new(title: String, kind: Kind, reply: oneshot::Sender<Option<Answer>>) -> Self {
        let (selected, text) = match &kind {
            Kind::Select { items, default } => {
                ((*default).min(items.len().saturating_sub(1)), String::new())
            }
            Kind::Input { default, .. } => (0, default.clone().unwrap_or_default()),
        };
        Self {
            title,
            kind,
            selected,
            text,
            reply: Some(reply),
        }
    }

    fn answer(&mut self, answer: Option<Answer>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(answer);
        }
    }
}

/// One line of the terminal pane
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogLine {
    pub time: String,
    pub stream: Stream,
    pub text: String,
}

/// What the loop must do after a key press
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    None,
    Start(Action),
    ToggleLanguage,
    Interrupt,
    Quit,
}

pub(crate) struct App {
    pub lang: Lang,
    pub menu: usize,
    pub dialog: Option<Dialog>,
    pub lines: VecDeque<LogLine>,
    /// Lines scrolled up from the bottom (0 = follow the output)
    pub scroll: usize,
    pub nas: Nas,
    pub config: Option<Config>,
    pub history: Vec<BackupManifest>,
    /// Flow currently running
    pub running: Option<Action>,
    /// Full-screen display left for an external program
    pub suspended: bool,
    /// Animation counter (spinners), advanced by the clock tick
    pub tick: usize,
}

impl App {
    pub(crate) fn new(lang: Lang) -> Self {
        Self {
            lang,
            menu: 0,
            dialog: None,
            lines: VecDeque::new(),
            scroll: 0,
            nas: Nas::Checking,
            config: None,
            history: Vec::new(),
            running: None,
            suspended: false,
            tick: 0,
        }
    }

    pub(crate) fn strings(&self) -> &'static Strings {
        self.lang.strings()
    }

    /// Append one line to the terminal pane (keeps the scroll position when scrolled up)
    pub(crate) fn push_line(&mut self, stream: Stream, text: String) {
        self.lines.push_back(LogLine {
            time: Local::now().format("%H:%M:%S").to_string(),
            stream,
            text,
        });
        if self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        } else if self.scroll > 0 {
            self.scroll += 1;
        }
    }

    pub(crate) fn info(&mut self, text: impl Into<String>) {
        self.push_line(Stream::Out, text.into());
    }

    /// Show a dialog asked by the flow (a previous one, if any, is cancelled)
    pub(crate) fn open_dialog(&mut self, dialog: Dialog) {
        if let Some(mut old) = self.dialog.take() {
            old.answer(None);
        }
        self.dialog = Some(dialog);
    }

    /// Cancel the open dialog (flow finished, console closing)
    pub(crate) fn cancel_dialog(&mut self) {
        if let Some(mut dialog) = self.dialog.take() {
            dialog.answer(None);
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> Command {
        let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'));
        if self.dialog.is_some() {
            self.dialog_key(key, ctrl_c);
            return Command::None;
        }
        if self.scroll_key(key.code) {
            return Command::None;
        }
        if self.running.is_some() {
            return if ctrl_c {
                Command::Interrupt
            } else {
                match key.code {
                    KeyCode::Char(c) if Action::from_key(c).is_some() => {
                        let s = self.strings();
                        self.info(s.busy_refused);
                        Command::None
                    }
                    _ => Command::None,
                }
            };
        }
        if ctrl_c {
            return Command::Quit;
        }
        let action = match key.code {
            KeyCode::Up => {
                self.menu = self.menu.checked_sub(1).unwrap_or(MENU.len() - 1);
                return Command::None;
            }
            KeyCode::Down => {
                self.menu = (self.menu + 1) % MENU.len();
                return Command::None;
            }
            KeyCode::Enter => MENU[self.menu],
            KeyCode::Esc => Action::Quit,
            KeyCode::Char(c) => match Action::from_key(c) {
                Some(action) => action,
                None => return Command::None,
            },
            _ => return Command::None,
        };
        if let Some(index) = MENU.iter().position(|a| *a == action) {
            self.menu = index;
        }
        match action {
            Action::Quit => Command::Quit,
            Action::Language => Command::ToggleLanguage,
            flow => Command::Start(flow),
        }
    }

    fn scroll_key(&mut self, code: KeyCode) -> bool {
        let max = self.lines.len().saturating_sub(1);
        match code {
            KeyCode::PageUp => self.scroll = (self.scroll + PAGE).min(max),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(PAGE),
            KeyCode::Home => self.scroll = max,
            KeyCode::End => self.scroll = 0,
            _ => return false,
        }
        true
    }

    fn dialog_key(&mut self, key: KeyEvent, ctrl_c: bool) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        if ctrl_c || key.code == KeyCode::Esc {
            self.cancel_dialog();
            return;
        }
        let answer = match (&dialog.kind, key.code) {
            (Kind::Select { items, .. }, KeyCode::Up) => {
                dialog.selected = dialog
                    .selected
                    .checked_sub(1)
                    .unwrap_or(items.len().saturating_sub(1));
                None
            }
            (Kind::Select { items, .. }, KeyCode::Down) => {
                dialog.selected = (dialog.selected + 1) % items.len().max(1);
                None
            }
            (Kind::Select { items, .. }, KeyCode::Enter) if !items.is_empty() => {
                Some(Answer::Index(dialog.selected))
            }
            (Kind::Input { .. }, KeyCode::Enter) => Some(Answer::Text(dialog.text.clone())),
            (Kind::Input { .. }, KeyCode::Backspace) => {
                dialog.text.pop();
                None
            }
            (Kind::Input { .. }, KeyCode::Char(c)) => {
                dialog.text.push(c);
                None
            }
            _ => None,
        };
        if answer.is_some()
            && let Some(mut dialog) = self.dialog.take()
        {
            dialog.answer(answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_c() -> KeyEvent {
        let mut k = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        k.kind = KeyEventKind::Press;
        k
    }

    #[test]
    fn shortcuts_map_to_actions() {
        for action in MENU {
            assert_eq!(Action::from_key(action.key()), Some(action));
        }
        assert_eq!(Action::from_key('l'), Some(Action::Language));
        assert_eq!(Action::from_key('x'), None);
        assert!(Action::Backup.is_flow());
        assert!(!Action::Quit.is_flow());
    }

    #[test]
    fn idle_keys() {
        let mut app = App::new(Lang::En);
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            Command::Start(Action::Backup)
        );
        assert_eq!(app.on_key(key(KeyCode::Up)), Command::None);
        assert_eq!(MENU[app.menu], Action::Quit); // wraps around
        assert_eq!(
            app.on_key(key(KeyCode::Char('v'))),
            Command::Start(Action::Verify)
        );
        assert_eq!(MENU[app.menu], Action::Verify);
        assert_eq!(app.on_key(key(KeyCode::Char('L'))), Command::ToggleLanguage);
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Command::Quit);
        assert_eq!(app.on_key(ctrl_c()), Command::Quit);
    }

    #[test]
    fn busy_keys_never_start_or_quit() {
        let mut app = App::new(Lang::Fr);
        app.running = Some(Action::Backup);
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Command::None);
        assert_eq!(app.on_key(key(KeyCode::Char('b'))), Command::None);
        assert_eq!(app.lines.len(), 2); // "already running" told twice
        assert_eq!(app.on_key(ctrl_c()), Command::Interrupt);
    }

    #[tokio::test]
    async fn select_dialog_answers_the_flow() {
        let mut app = App::new(Lang::En);
        let (tx, rx) = oneshot::channel();
        let kind = Kind::Select {
            items: vec!["a".into(), "b".into(), "c".into()],
            default: 1,
        };
        app.open_dialog(Dialog::new("t".into(), kind, tx));
        app.on_key(key(KeyCode::Down));
        app.on_key(key(KeyCode::Down)); // wraps to the first item
        app.on_key(key(KeyCode::Enter));
        assert!(app.dialog.is_none());
        assert_eq!(rx.await.unwrap(), Some(Answer::Index(0)));
    }

    #[tokio::test]
    async fn input_dialog_edits_and_cancels() {
        let mut app = App::new(Lang::En);
        let (tx, rx) = oneshot::channel();
        let kind = Kind::Input {
            default: Some("~/x".into()),
            masked: false,
        };
        app.open_dialog(Dialog::new("t".into(), kind, tx));
        app.on_key(key(KeyCode::Backspace));
        app.on_key(key(KeyCode::Char('y')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(rx.await.unwrap(), Some(Answer::Text("~/y".into())));

        let (tx, rx) = oneshot::channel();
        let kind = Kind::Input {
            default: None,
            masked: true,
        };
        app.open_dialog(Dialog::new("t".into(), kind, tx));
        app.on_key(key(KeyCode::Char('s')));
        app.on_key(ctrl_c()); // Ctrl-C in a dialog cancels it, never quits
        assert!(app.dialog.is_none());
        assert_eq!(rx.await.unwrap(), None);
    }

    #[test]
    fn scrolling_keeps_position_while_output_arrives() {
        let mut app = App::new(Lang::En);
        for i in 0..30 {
            app.info(format!("line {i}"));
        }
        app.on_key(key(KeyCode::PageUp));
        assert_eq!(app.scroll, PAGE);
        app.info("new");
        assert_eq!(app.scroll, PAGE + 1);
        app.on_key(key(KeyCode::End));
        assert_eq!(app.scroll, 0);
        app.on_key(key(KeyCode::Home));
        assert_eq!(app.scroll, app.lines.len() - 1);
    }
}
