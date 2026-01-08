use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEvent},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::text::{Line, Text};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph},
};

use crate::bencode::MetaInfo;
use crate::tui::app_state::{AppState, PeerStatus, PieceState};

/// Main TUI run loop
pub fn run(state: Arc<RwLock<AppState>>, info: MetaInfo) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();
    let mut peer_scroll: usize = 0;
    let mut files_scroll: usize = 0;

    loop {
        terminal.draw(|f| draw_ui(f, &state, peer_scroll, files_scroll, &info))?;

        // Event handling
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let CEvent::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') => break, // quit
                    KeyCode::Up => peer_scroll = peer_scroll.saturating_sub(3),
                    KeyCode::Down => peer_scroll = peer_scroll.saturating_add(3),
                    KeyCode::Char('p') => files_scroll = files_scroll.saturating_sub(1),
                    KeyCode::Char('n') => files_scroll = files_scroll.saturating_add(1),
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Draw entire UI
fn draw_ui(
    f: &mut ratatui::Frame,
    state: &Arc<RwLock<AppState>>,
    peer_scroll: usize,
    files_scroll: usize,
    info: &MetaInfo,
) {
    let app = state.read().unwrap();
    let area = f.area();

    let cols = area.width.max(1) as usize;
    let rows = (app.pieces.len() + cols - 1) / cols;
    let piece_height = rows as u16 + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Max(5),
            Constraint::Length(piece_height),
            Constraint::Min(5),
        ])
        .split(area);

    draw_torrent_info(f, chunks[0], info, files_scroll);
    draw_piece_map(f, chunks[1], &app.pieces);
    draw_peer_panel(f, chunks[2], &app.peers, peer_scroll);
}

fn format_file_line(name: &str, length: u64, max_width: usize) -> String {
    let size_str = bytesize::ByteSize(length).to_string();
    let available = max_width.saturating_sub(size_str.len() + 2); // space for ": "

    if name.len() > available {
        let truncated = &name[..available];
        format!("{}: {}", truncated, size_str)
    } else {
        format!("{}: {}", name, size_str)
    }
}

fn draw_torrent_info(f: &mut ratatui::Frame, area: Rect, info: &MetaInfo, files_scroll: usize) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(50); 2])
        .split(area);

    let b = Block::new().title("Torrent").borders(Borders::ALL);
    let para = Paragraph::new(vec![
        Line::from(format!(
            "Comment: {}",
            info.comment.clone().unwrap_or("".to_string())
        )),
        Line::from(format!(
            "Created By: {}",
            info.created_by.clone().unwrap_or("".to_string())
        )),
        Line::from(format!("Created On: {}", info.creation_date.unwrap_or(0))),
    ])
    .block(b);
    f.render_widget(para, layout[0]);
    let b = Block::new().title("Files").borders(Borders::ALL);
    let w = layout[1].width - 2;
    let para = match &info.info.mode {
        crate::bencode::FileMode::SingleFile { length } => {
            let line = format_file_line(&info.info.name, *length, w as usize);
            Paragraph::new(vec![Line::from(line)]).block(b)
        }
        crate::bencode::FileMode::MultipleFiles { files } => {
            let mut lines = Vec::new();
            let start = files_scroll.min(files.len().saturating_sub(1));
            for file in files.iter().skip(start) {
                let name = file.path.join("/");
                let line = format_file_line(&name, file.length, w as usize);
                lines.push(Line::from(line));
            }
            Paragraph::new(lines).block(b)
        }
    };
    f.render_widget(para, layout[1]);
}

fn draw_piece_map(f: &mut ratatui::Frame, area: Rect, pieces: &[PieceState]) {
    if pieces.is_empty() {
        return;
    }

    let cols = area.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut current_line: Vec<Span> = Vec::new();

    for (i, p) in pieces.iter().enumerate() {
        let (ch, color) = match p {
            PieceState::Missing => ('·', Color::DarkGray),
            PieceState::Requested => ('░', Color::Yellow),
            PieceState::Downloading => ('▒', Color::Cyan),
            PieceState::Complete => ('█', Color::Green),
        };

        current_line.push(Span::styled(ch.to_string(), Style::default().fg(color)));

        if (i + 1) % cols == 0 {
            lines.push(Line::from(current_line));
            current_line = Vec::new();
        }
    }

    // Push remaining line if any
    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Pieces"));

    f.render_widget(paragraph, area);
}

fn draw_peer_panel(
    f: &mut ratatui::Frame,
    area: Rect,
    peers: &VecDeque<PeerStatus>,
    scroll: usize,
) {
    // outer container
    let outer = Block::default().borders(Borders::ALL).title("Peers");
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    if peers.is_empty() {
        return;
    }
    let start = scroll.min(peers.len().saturating_sub(1));
    let visible: Vec<&PeerStatus> = peers.iter().skip(start).take(6).collect();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    for row in 0..2 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(rows[row]);

        for col in 0..3 {
            let idx = row * 3 + col;
            if let Some(peer) = visible.get(idx) {
                draw_peer_box(f, cols[col], peer);
            }
        }
    }
}

fn draw_peer_box(f: &mut ratatui::Frame, area: Rect, peer: &PeerStatus) {
    // let status = if peer.choked { "Choked" } else { "Unchoked" };

    let height = area.height.saturating_sub(2);
    let start = peer.task.len().saturating_sub(height.into());

    let mut text = Vec::new();
    for task in peer.task.iter().skip(start) {
        text.push(Line::from(task.clone()));
    }
    // let text = vec![Line::from(peer.task.clone())];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(peer.peer_id.clone());

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}
