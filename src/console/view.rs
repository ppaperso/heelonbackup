//! Dashboard rendering (ratatui): header, NAS and last-backup panels, actions,
//! terminal pane with live progress, key help, modal dialogs

use super::app::{App, MENU, Nas};
use super::prompt::Kind;
use super::strings::{Lang, Strings};
use crate::config::{Environment, SmbTarget};
use crate::service::status::STALE_DAYS;
use crate::storage::{BackupManifest, BackupStatus};
use crate::ui::{self, Stream};
use chrono::Local;
use indicatif::ProgressBar;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// Progress lines shown at most under the terminal pane
const MAX_BARS: usize = 3;

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    let s = app.strings();
    let [header, panels, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    let [nas, last] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(panels);
    draw_nas(frame, nas, app, s);
    draw_last(frame, last, app, s);
    let [actions, terminal] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(30)]).areas(body);
    draw_actions(frame, actions, app, s);
    draw_terminal(frame, terminal, app, s);
    draw_footer(frame, footer, app, s);
    if app.dialog.is_some() {
        draw_dialog(frame, app, s);
    }
}

fn panel(title: &str) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().bold().fg(Color::Cyan),
        ))
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let s = app.strings();
    let bar = Style::new().bg(Color::Blue).fg(Color::White);
    frame.render_widget(Block::new().style(bar), area);

    let (env, env_style) = match Environment::detect() {
        Environment::Dev => (
            s.env_dev,
            Style::new().bg(Color::Red).fg(Color::White).bold(),
        ),
        Environment::Prod => (
            s.env_prod,
            Style::new().bg(Color::Green).fg(Color::Black).bold(),
        ),
    };
    let lang = |l: Lang, label: &'static str| {
        if l == app.lang {
            Span::styled(label, Style::new().bg(Color::White).fg(Color::Blue).bold())
        } else {
            Span::raw(label)
        }
    };
    let left = Line::from(vec![
        Span::styled(" HeelonBackup ", Style::new().bold()),
        Span::raw(format!("v{VERSION}  ")),
        Span::styled(format!(" {env} "), env_style),
    ]);
    let right = Line::from(vec![
        Span::raw(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
        Span::raw("   "),
        lang(Lang::En, " EN "),
        Span::raw("|"),
        lang(Lang::Fr, " FR "),
        Span::raw(" "),
    ]);
    frame.render_widget(Paragraph::new(left).style(bar), area);
    frame.render_widget(
        Paragraph::new(right).style(bar).alignment(Alignment::Right),
        area,
    );
}

fn field(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {label:<13}"), Style::new().fg(Color::DarkGray)),
        Span::raw(value.into()),
    ])
}

fn draw_nas(frame: &mut Frame, area: Rect, app: &App, s: &Strings) {
    let state = match &app.nas {
        Nas::Checking => Line::from(vec![
            Span::styled(" ● ", Style::new().fg(Color::Yellow)),
            Span::raw(s.nas_checking),
        ]),
        Nas::Online { address, latency } => Line::from(vec![
            Span::styled(" ● ", Style::new().fg(Color::Green)),
            Span::styled(s.nas_online, Style::new().fg(Color::Green).bold()),
            Span::raw(format!("  {address} · {} ms", latency.as_millis())),
        ]),
        Nas::Offline { address, reason } => Line::from(vec![
            Span::styled(" ● ", Style::new().fg(Color::Red)),
            Span::styled(s.nas_offline, Style::new().fg(Color::Red).bold()),
            Span::raw(format!("  {address} · {reason}")),
        ]),
        Nas::NoConfig => Line::from(vec![
            Span::styled(" ● ", Style::new().fg(Color::DarkGray)),
            Span::raw(s.nas_no_config),
        ]),
    };
    let mut lines = vec![state];
    if let Some(config) = &app.config {
        let on_off = |b: bool| if b { s.on } else { s.off };
        let share = SmbTarget::parse(&config.smb.url)
            .map(|t| t.to_string())
            .unwrap_or_else(|_| config.smb.url.clone());
        lines.push(field(s.field_share, share));
        if let Ok(base) = config.remote_base() {
            lines.push(field(s.field_folder, base));
        }
        // the sources themselves (the line is cut at the panel edge; full list via
        // Configuration › sources, or at the start of a backup)
        let sources = &config.backup.sources;
        lines.push(field(
            s.field_sources,
            format!("({}) {}", sources.len(), sources.join(", ")),
        ));
        let retention = match config.storage.retention {
            0 => s.keep_all.to_string(),
            n => n.to_string(),
        };
        lines.push(field(s.field_retention, retention));
        lines.push(field(s.field_encryption, on_off(config.smb.encrypt)));
    }
    frame.render_widget(Paragraph::new(lines).block(panel(s.panel_nas)), area);
}

fn status_style(status: BackupStatus) -> Style {
    match status {
        BackupStatus::Completed => Style::new().fg(Color::Green).bold(),
        BackupStatus::CompletedWithErrors => Style::new().fg(Color::Yellow).bold(),
        BackupStatus::Failed => Style::new().fg(Color::Red).bold(),
        BackupStatus::InProgress => Style::new().fg(Color::Yellow),
    }
}

fn age_secs(m: &BackupManifest) -> i64 {
    (Local::now() - m.started_at).num_seconds()
}

fn draw_last(frame: &mut Frame, area: Rect, app: &App, s: &Strings) {
    let Some(last) = app.history.first() else {
        let text = Paragraph::new(Line::from(format!(" {}", s.no_history)).fg(Color::Yellow));
        frame.render_widget(text.block(panel(s.panel_last)), area);
        return;
    };
    let partial = if last.partial {
        format!(" ({})", s.partial)
    } else {
        String::new()
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {:<13}", s.field_status),
                Style::new().fg(Color::DarkGray),
            ),
            Span::styled(s.status(last.status), status_style(last.status)),
            Span::raw(partial),
        ]),
        field(
            s.field_date,
            format!("{} ({})", ui::date(last.started_at), s.age(age_secs(last))),
        ),
        field(
            s.field_size,
            format!(
                "{} · {} {}",
                ui::bytes(last.total_size),
                last.file_count,
                s.files
            ),
        ),
    ];
    if let Some(d) = last.duration() {
        lines.push(field(s.field_duration, ui::duration(d)));
    }
    match app.history.iter().find(|m| m.status.is_usable()) {
        Some(ok) if (Local::now() - ok.started_at).num_days() >= STALE_DAYS => {
            lines.push(Line::from(format!(" ⚠ {}", s.stale_warning)).fg(Color::Yellow));
        }
        Some(ok) if ok.name != last.name => lines.push(field(
            s.field_last_ok,
            format!("{} ({})", ok.name, s.age(age_secs(ok))),
        )),
        Some(_) => {}
        None => lines.push(Line::from(format!(" ✖ {}", s.status_failed)).fg(Color::Red)),
    }
    frame.render_widget(Paragraph::new(lines).block(panel(s.panel_last)), area);
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &App, s: &Strings) {
    let busy = app.running.is_some();
    let items: Vec<ListItem> = MENU
        .iter()
        .map(|action| {
            let style = if busy && action.is_flow() {
                Style::new().fg(Color::DarkGray)
            } else {
                Style::new()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", action.key()),
                    Style::new().fg(Color::Cyan).bold(),
                ),
                Span::raw(action.label(s)),
            ]))
            .style(style)
        })
        .collect();
    let list = List::new(items)
        .block(panel(s.panel_actions))
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶");
    let mut state = ListState::default().with_selected(Some(app.menu));
    frame.render_stateful_widget(list, area, &mut state);
}

fn line_style(stream: Stream, text: &str) -> Style {
    let t = text.trim_start();
    if t.starts_with('❌') || t.starts_with('✖') || t.starts_with('🔴') || t.contains(" ERROR ")
    {
        Style::new().fg(Color::Red)
    } else if t.starts_with('⚠') || t.starts_with('⏹') || t.contains(" WARN ") {
        Style::new().fg(Color::Yellow)
    } else if t.starts_with('✅') || t.starts_with('🟢') || t.starts_with('✔') {
        Style::new().fg(Color::Green)
    } else if stream == Stream::Err {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new()
    }
}

fn draw_terminal(frame: &mut Frame, area: Rect, app: &App, s: &Strings) {
    let mut title = s.panel_terminal.to_string();
    if let Some(action) = app.running {
        title = format!(
            "{title} — {} {} {}",
            SPINNER[app.tick % SPINNER.len()],
            s.running,
            action.label(s)
        );
    }
    let block = panel(&title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let bars: Vec<ProgressBar> = ui::active_bars().into_iter().take(MAX_BARS).collect();
    let [log_area, bars_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(bars.len() as u16)]).areas(inner);

    if app.lines.is_empty() {
        let hint = Paragraph::new(Line::from(format!(" {}", s.terminal_empty)).fg(Color::DarkGray));
        frame.render_widget(hint, log_area);
    } else {
        let height = log_area.height as usize;
        let end = app.lines.len().saturating_sub(app.scroll);
        let start = end.saturating_sub(height);
        let lines: Vec<Line> = app
            .lines
            .range(start..end)
            .map(|l| {
                Line::from(vec![
                    Span::styled(format!("{} ", l.time), Style::new().fg(Color::DarkGray)),
                    Span::styled(l.text.clone(), line_style(l.stream, &l.text)),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), log_area);
        if app.scroll > 0 {
            let more = format!(" ↓ {} ", app.scroll);
            let w = more.chars().count() as u16;
            let r = Rect::new(
                log_area.right().saturating_sub(w),
                log_area.bottom().saturating_sub(1),
                w,
                1,
            );
            frame.render_widget(Paragraph::new(more).reversed(), r);
        }
    }

    for (i, pb) in bars.iter().enumerate() {
        let r = Rect::new(bars_area.x, bars_area.y + i as u16, bars_area.width, 1);
        draw_bar(frame, r, pb, app.tick);
    }
}

fn draw_bar(frame: &mut Frame, area: Rect, pb: &ProgressBar, tick: usize) {
    match pb.length() {
        Some(total) if total > 0 => {
            let pos = pb.position().min(total);
            let ratio = pos as f64 / total as f64;
            let eta = if pos > 0 {
                ui::duration(pb.eta())
            } else {
                "…".into()
            };
            let label = format!(
                "{} {:.0}% · {}/{} · {}/s · ETA {} · {}",
                pb.prefix(),
                ratio * 100.0,
                ui::bytes(pos),
                ui::bytes(total),
                ui::bytes(pb.per_sec() as u64),
                eta,
                pb.message()
            );
            let gauge = Gauge::default()
                .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Black))
                .ratio(ratio)
                .label(Span::styled(label, Style::new().fg(Color::White).bold()));
            frame.render_widget(gauge, area);
        }
        _ => {
            let text = format!(
                " {} {} [{}]",
                SPINNER[tick % SPINNER.len()],
                pb.message(),
                ui::duration(pb.elapsed())
            );
            frame.render_widget(Paragraph::new(text).fg(Color::Cyan), area);
        }
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, s: &Strings) {
    let keys = if app.dialog.is_some() {
        s.keys_prompt
    } else if app.running.is_some() {
        s.keys_busy
    } else {
        s.keys_idle
    };
    frame.render_widget(Paragraph::new(format!(" {keys}")).fg(Color::DarkGray), area);
}

/// Rectangle of `width` x `height` centred in `area` (clamped to it)
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn draw_dialog(frame: &mut Frame, app: &App, s: &Strings) {
    let Some(dialog) = &app.dialog else {
        return;
    };
    let width = (dialog.title.chars().count() as u16 + 8).clamp(50, 90);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(Color::Cyan))
        .padding(Padding::horizontal(1))
        .title(Span::styled(" HeelonBackup ", Style::new().bold()))
        .title_bottom(Line::from(format!(" {} ", s.keys_prompt)).fg(Color::DarkGray));
    match &dialog.kind {
        Kind::Select { items, .. } => {
            let visible = items.len().min(12) as u16;
            let area = centered(frame.area(), width, visible + 4);
            frame.render_widget(Clear, area);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let [title, list_area] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
            frame.render_widget(
                Paragraph::new(Line::from(dialog.title.clone()).bold()),
                title,
            );
            let list = List::new(items.iter().map(|i| ListItem::new(format!(" {i}"))))
                .highlight_style(Style::new().bg(Color::Cyan).fg(Color::Black).bold())
                .highlight_symbol("❯");
            let mut state = ListState::default().with_selected(Some(dialog.selected));
            frame.render_stateful_widget(list, list_area, &mut state);
        }
        Kind::Input { masked, .. } => {
            let area = centered(frame.area(), width, 6);
            frame.render_widget(Clear, area);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let [title, field_area] =
                Layout::vertical([Constraint::Length(2), Constraint::Length(1)]).areas(inner);
            frame.render_widget(
                Paragraph::new(Line::from(dialog.title.clone()).bold()),
                title,
            );
            let shown = if *masked {
                "•".repeat(dialog.text.chars().count())
            } else {
                dialog.text.clone()
            };
            // keep the end of a long input visible
            let room = field_area.width.saturating_sub(4) as usize;
            let skip = shown.chars().count().saturating_sub(room);
            let visible: String = shown.chars().skip(skip).collect();
            let field = Paragraph::new(Line::from(vec![
                Span::styled("❯ ", Style::new().fg(Color::Cyan)),
                Span::raw(visible.clone()),
                Span::styled(
                    "▏",
                    Style::new()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ]));
            frame.render_widget(field, field_area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::app::Dialog;
    use crate::console::prompt::Kind;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tokio::sync::oneshot;

    fn render(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn dashboard_shows_header_panels_and_menu_in_both_languages() {
        let mut app = App::new(Lang::En);
        app.nas = Nas::Online {
            address: "192.168.1.100:445".into(),
            latency: std::time::Duration::from_millis(3),
        };
        let screen = render(&app);
        assert!(screen.contains("HeelonBackup"));
        assert!(screen.contains(&format!("v{VERSION}")));
        assert!(screen.contains("ONLINE"));
        assert!(screen.contains("192.168.1.100:445"));
        assert!(screen.contains("Start a backup"));
        assert!(screen.contains("No backup recorded"));

        app.lang = Lang::Fr;
        let screen = render(&app);
        assert!(screen.contains("EN LIGNE"));
        assert!(screen.contains("Lancer une sauvegarde"));
    }

    #[test]
    fn masked_dialog_never_shows_the_password() {
        let mut app = App::new(Lang::En);
        let (tx, _rx) = oneshot::channel();
        let kind = Kind::Input {
            default: None,
            masked: true,
        };
        let mut dialog = Dialog::new("SMB password for user@nas".into(), kind, tx);
        dialog.text = "s3cret".into();
        app.open_dialog(dialog);
        let screen = render(&app);
        assert!(screen.contains("SMB password for user@nas"));
        assert!(screen.contains("••••••"));
        assert!(!screen.contains("s3cret"));
    }

    #[test]
    fn transfer_bar_shows_percentage_and_sizes() {
        let pb = ProgressBar::hidden();
        pb.set_length(1000);
        pb.set_position(500);
        pb.set_prefix("Uploading");
        pb.set_message("3/6 files");
        let mut terminal = Terminal::new(TestBackend::new(100, 1)).unwrap();
        terminal.draw(|f| draw_bar(f, f.area(), &pb, 0)).unwrap();
        let line: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(line.contains("Uploading 50%"), "{line}");
        assert!(line.contains("500 B/1000 B"), "{line}");
        assert!(line.contains("3/6 files"), "{line}");
    }

    #[test]
    fn terminal_pane_shows_the_latest_lines() {
        let mut app = App::new(Lang::En);
        for i in 0..100 {
            app.info(format!("output line {i}"));
        }
        let screen = render(&app);
        assert!(screen.contains("output line 99"));
        assert!(!screen.contains("output line 10 "));
    }
}
