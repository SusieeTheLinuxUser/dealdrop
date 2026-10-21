//! Small terminal view over the same public, no-account sources used by the desktop poll.

use crate::sources::{self, Deal, Free};
use crate::stores;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::{io, process::Command, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Quit,
    Refresh,
    Up,
    Down,
    NextTab,
    PreviousTab,
    SelectTab(usize),
    Open,
    Ignore,
}

fn key_action(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Tab | KeyCode::Char('l') => Action::NextTab,
        KeyCode::BackTab | KeyCode::Char('h') => Action::PreviousTab,
        KeyCode::Char('1') => Action::SelectTab(0),
        KeyCode::Char('2') => Action::SelectTab(1),
        KeyCode::Char('3') => Action::SelectTab(2),
        KeyCode::Enter => Action::Open,
        _ => Action::Ignore,
    }
}

#[derive(Clone, Debug, Default)]
struct Row {
    title: String,
    store: String,
    detail: String,
    url: String,
}

fn free_row(item: &Free) -> Row {
    let detail = item
        .end_date
        .as_deref()
        .map(|date| format!("ends {date}"))
        .unwrap_or_default();
    Row {
        title: item.title.clone(),
        store: item.source.clone(),
        detail,
        url: item.url.clone(),
    }
}

fn deal_row(item: &Deal) -> Row {
    Row {
        title: item.title.clone(),
        store: item.shop.clone(),
        detail: format!("${:.2} · {}% off", item.price, item.cut),
        url: item.url.clone(),
    }
}

fn row_text(row: &Row) -> String {
    if row.detail.is_empty() {
        format!("{}  [{}]", row.title, row.store)
    } else {
        format!("{}  [{}]  {}", row.title, row.store, row.detail)
    }
}

#[derive(Default)]
struct Data {
    free: Vec<Row>,
    deals: Vec<Row>,
    sales: Vec<Row>,
    errors: Vec<String>,
}

impl Data {
    fn rows(&self, tab: usize) -> &[Row] {
        match tab {
            0 => &self.free,
            1 => &self.deals,
            _ => &self.sales,
        }
    }

    async fn refresh(&mut self) {
        let (epic, steam, keep, weekends, gamerpower, hot, steam_sales, gog_sales, epic_sales) = tokio::join!(
            sources::epic_free(),
            sources::steam_free_featured(),
            sources::steam_free_to_keep(),
            sources::steam_free_weekends(),
            sources::gamerpower(),
            sources::cheapshark_deals(),
            sources::steam_sales(),
            sources::gog_sales(),
            stores::epic_sales(),
        );
        let mut errors = Vec::new();
        let free_errors_before = errors.len();
        let mut free_items = Vec::new();
        match epic {
            Ok((now, _)) => free_items.extend(now),
            Err(e) => errors.push(format!("Epic free games: {e}")),
        }
        match steam {
            Ok(items) => free_items.extend(items),
            Err(e) => errors.push(format!("Steam free games: {e}")),
        }
        match keep {
            Ok(items) => free_items.extend(items),
            Err(e) => errors.push(format!("Steam free to keep: {e}")),
        }
        match gamerpower {
            Ok(items) => free_items.extend(items),
            Err(e) => errors.push(format!("GamerPower giveaways: {e}")),
        }
        match weekends {
            Ok(items) => free_items.extend(items),
            Err(e) => errors.push(format!("Steam free weekends: {e}")),
        }
        if errors.len() == free_errors_before || self.free.is_empty() {
            self.free = crate::dedupe_free(free_items)
                .iter()
                .map(free_row)
                .collect();
        }

        match hot {
            Ok(items) => self.deals = items.iter().map(deal_row).collect(),
            Err(e) => errors.push(format!("Hot deals: {e}")),
        }
        let mut sales = std::mem::take(&mut self.sales);
        for (name, store, result) in [
            ("Steam sales", "Steam", steam_sales),
            ("GOG sales", "GOG", gog_sales),
            ("Epic sales", "Epic", epic_sales),
        ] {
            match result {
                Ok(items) => {
                    sales.retain(|row| row.store != store);
                    sales.extend(items.iter().map(deal_row));
                }
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        self.sales = sales;
        self.errors = errors;
    }
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("--tui needs an interactive terminal".into());
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let _session = TerminalSession::enter()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut data = Data::default();
    runtime.block_on(data.refresh());
    let mut tab = 0usize;
    let mut selected = [0usize; 3];
    let mut status = "Loaded public store data".to_string();

    loop {
        terminal.draw(|frame| draw(frame, &data, tab, selected[tab], &status))?;
        if !event::poll(Duration::from_millis(150))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        match key_action(key) {
            Action::Quit => break,
            Action::Refresh => {
                status = "Refreshing…".into();
                terminal.draw(|frame| draw(frame, &data, tab, selected[tab], &status))?;
                runtime.block_on(data.refresh());
                status = if data.errors.is_empty() {
                    "Updated".into()
                } else {
                    "Some sources failed; showing available data".into()
                };
                for i in 0..3 {
                    selected[i] = selected[i].min(data.rows(i).len().saturating_sub(1));
                }
            }
            Action::Up => selected[tab] = selected[tab].saturating_sub(1),
            Action::Down => {
                selected[tab] = (selected[tab] + 1).min(data.rows(tab).len().saturating_sub(1))
            }
            Action::NextTab => tab = (tab + 1) % 3,
            Action::PreviousTab => tab = (tab + 2) % 3,
            Action::SelectTab(next) => tab = next,
            Action::Open => {
                if let Some(row) = data.rows(tab).get(selected[tab]) {
                    status = match open_link(&row.url) {
                        Ok(()) => "Opened in browser".into(),
                        Err(e) => format!("Could not open link: {e}"),
                    };
                }
            }
            Action::Ignore => {}
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, data: &Data, tab: usize, selected: usize, status: &str) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);
    let titles = ["1 Free now", "2 Hot deals", "3 Store sales"];
    let spans = titles
        .iter()
        .enumerate()
        .flat_map(|(i, title)| {
            let style = if i == tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            [Span::styled(format!(" {title} "), style), Span::raw("  ")]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .title("DealDrop · Tab or h/l switches · q quits")
                .borders(Borders::ALL),
        ),
        chunks[0],
    );
    let rows = data.rows(tab);
    let items = rows
        .iter()
        .map(|row| ListItem::new(row_text(row)))
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!(
                        "{} · {} items",
                        titles[tab].split_once(' ').map_or(titles[tab], |(_, s)| s),
                        rows.len()
                    ))
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
            .highlight_symbol("› "),
        chunks[1],
        &mut state,
    );
    let error = data.errors.first().map(String::as_str).unwrap_or("");
    let text = if error.is_empty() {
        format!("{status}  |  ↑/↓ or j/k move · Enter opens · r refreshes")
    } else {
        format!("{status}  |  {error}")
    };
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );
}

fn open_link(raw: &str) -> Result<(), String> {
    let url =
        crate::safe_https(raw).ok_or_else(|| "link rejected by HTTPS validation".to_string())?;
    open_validated_url(url.as_str())
}

#[cfg(target_os = "linux")]
fn open_validated_url(url: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn open_validated_url(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn open_validated_url(url: &str) -> Result<(), String> {
    Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn open_validated_url(_: &str) -> Result<(), String> {
    Err("browser opening is unsupported on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_bindings_map_to_actions() {
        assert_eq!(key_action(KeyEvent::from(KeyCode::Char('j'))), Action::Down);
        assert_eq!(
            key_action(KeyEvent::from(KeyCode::Char('3'))),
            Action::SelectTab(2)
        );
        assert_eq!(key_action(KeyEvent::from(KeyCode::Enter)), Action::Open);
        assert_eq!(key_action(KeyEvent::from(KeyCode::Esc)), Action::Quit);
    }

    #[test]
    fn row_format_includes_store_and_price() {
        let row = Row {
            title: "Example Game".into(),
            store: "Steam".into(),
            detail: "$4.99 · 50% off".into(),
            url: String::new(),
        };
        assert_eq!(row_text(&row), "Example Game  [Steam]  $4.99 · 50% off");
    }
}
