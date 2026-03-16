//! Terminal document viewer with ratatui.
//!
//! Features: scroll, section navigation, search highlight.
//! Falls back to plain text if stdout is not a TTY.

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io;

/// Display document in TUI viewer. Returns when user presses 'q'.
pub fn view_document(title: &str, content: &str, highlight: Option<&str>) -> io::Result<()> {
    // Check if TTY
    if !atty_check() {
        // Not a terminal — print plain text
        println!("{content}");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let lines = build_lines(content, highlight);
    let sections = find_sections(content);
    let total_lines = lines.len();
    let mut scroll: u16 = 0;
    let mut current_section = 0;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(f.area());

            let paragraph = Paragraph::new(lines.clone())
                .block(
                    Block::default()
                        .title(format!(" {} ", title))
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0));

            f.render_widget(paragraph, chunks[0]);

            // Status bar
            let status = format!(
                " Line {}/{} | Sections: {} | q:quit j/k:scroll g/G:top/bottom n/N:next/prev section",
                scroll + 1,
                total_lines,
                sections.len()
            );
            let status_line = Line::from(Span::styled(
                status,
                Style::default().bg(Color::DarkGray).fg(Color::White),
            ));
            let status_bar = Paragraph::new(status_line);
            f.render_widget(status_bar, chunks[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            let page_size = terminal.size()?.height.saturating_sub(3);
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => break,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                    scroll = scroll
                        .saturating_add(1)
                        .min(total_lines.saturating_sub(1) as u16);
                }
                (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                    scroll = scroll.saturating_sub(1);
                }
                (KeyCode::PageDown, _) | (KeyCode::Char(' '), _) => {
                    scroll = scroll
                        .saturating_add(page_size)
                        .min(total_lines.saturating_sub(1) as u16);
                }
                (KeyCode::PageUp, _) => {
                    scroll = scroll.saturating_sub(page_size);
                }
                (KeyCode::Char('g'), _) | (KeyCode::Home, _) => scroll = 0,
                (KeyCode::Char('G'), _) | (KeyCode::End, _) => {
                    scroll = total_lines.saturating_sub(1) as u16;
                }
                (KeyCode::Char('n'), _) => {
                    // Next section
                    if !sections.is_empty() {
                        current_section = (current_section + 1).min(sections.len() - 1);
                        scroll = sections[current_section] as u16;
                    }
                }
                (KeyCode::Char('N'), _) => {
                    // Previous section
                    if !sections.is_empty() {
                        current_section = current_section.saturating_sub(1);
                        scroll = sections[current_section] as u16;
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn build_lines<'a>(content: &'a str, highlight: Option<&'a str>) -> Vec<Line<'a>> {
    content
        .lines()
        .map(|line| {
            if line.starts_with("# ") {
                Line::from(Span::styled(
                    line,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if line.starts_with("## ") || line.starts_with("### ") {
                Line::from(Span::styled(
                    line,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if let Some(query) = highlight {
                // Highlight search matches
                highlight_line(line, query)
            } else {
                Line::from(line)
            }
        })
        .collect()
}

fn highlight_line<'a>(line: &'a str, query: &'a str) -> Line<'a> {
    if query.is_empty() || !line.to_lowercase().contains(&query.to_lowercase()) {
        return Line::from(line);
    }

    let lower = line.to_lowercase();
    let query_lower = query.to_lowercase();
    let mut spans = Vec::new();
    let mut last_end = 0;

    for (pos, _) in lower.match_indices(&query_lower) {
        if pos > last_end {
            spans.push(Span::raw(&line[last_end..pos]));
        }
        spans.push(Span::styled(
            &line[pos..pos + query.len()],
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ));
        last_end = pos + query.len();
    }
    if last_end < line.len() {
        spans.push(Span::raw(&line[last_end..]));
    }

    Line::from(spans)
}

fn find_sections(content: &str) -> Vec<usize> {
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with("## ") || line.starts_with("# "))
        .map(|(i, _)| i)
        .collect()
}

fn atty_check() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
