pub mod chat_buffer;
pub mod input_line;
pub mod nick_list;
pub mod status_bar;
pub mod title_bar;

use std::collections::VecDeque;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, GenerationKind};
use crate::split::SplitDirection;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // title bar
            Constraint::Min(1),    // main area (chat + optional nick list / split)
            Constraint::Length(1),  // input line
            Constraint::Length(1),  // status bar
        ])
        .split(frame.area());

    title_bar::render(frame, chunks[0], app, theme);

    if let Some(ref gen) = app.pending_generation {
        // Show generation preview in split pane
        let split_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(chunks[1]);

        // Left: normal chat
        chat_buffer::render(frame, split_chunks[0], app, theme);

        // Separator
        render_separator(frame, split_chunks[1], SplitDirection::Vertical, theme);

        // Right: generated content preview
        render_generation_preview(frame, split_chunks[2], gen, theme);
    } else if let Some(ref split) = app.split {
        // Split mode: two chat panes, no nick list
        let direction = match split.direction {
            SplitDirection::Vertical => Direction::Horizontal,
            SplitDirection::Horizontal => Direction::Vertical,
        };

        // Leave 1 char/line for separator
        let split_chunks = Layout::default()
            .direction(direction)
            .constraints([
                Constraint::Percentage(split.ratio),
                Constraint::Length(1),       // separator
                Constraint::Percentage(100 - split.ratio),
            ])
            .split(chunks[1]);

        // Primary pane (active buffer)
        chat_buffer::render(frame, split_chunks[0], app, theme);

        // Separator
        render_separator(frame, split_chunks[1], split.direction, theme);

        // Secondary pane
        let empty = VecDeque::new();
        let messages = app.split_messages().unwrap_or(&empty);
        let scroll = app.split_scroll_offset();
        let search = app.split_search();
        chat_buffer::render_buffer(
            frame,
            split_chunks[2],
            messages,
            scroll,
            search,
            &app.timestamp_format,
            theme,
        );
    } else {
        // Single buffer mode with optional nick list
        let show_nick_list = app
            .active_server_state()
            .map(|ss| {
                ss.active_buffer.starts_with('#') && !ss.active_buf().nicks.is_empty()
            })
            .unwrap_or(false);

        if show_nick_list {
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Min(1),        // chat buffer
                    Constraint::Length(18),     // nick list
                ])
                .split(chunks[1]);

            chat_buffer::render(frame, main_chunks[0], app, theme);
            nick_list::render(frame, main_chunks[1], app, theme);
        } else {
            chat_buffer::render(frame, chunks[1], app, theme);
        }
    }

    input_line::render(frame, chunks[2], app, theme);
    status_bar::render(frame, chunks[3], app, theme);
}

/// Render a preview of generated content.
fn render_generation_preview(
    frame: &mut Frame,
    area: Rect,
    gen: &crate::app::PendingGeneration,
    theme: &Theme,
) {
    let header = match gen.kind {
        GenerationKind::Script => {
            let lang = gen.language.as_deref().unwrap_or("lua");
            format!(" Generated script ({}) — {}", lang, gen.name)
        }
        GenerationKind::Theme => format!(" Generated theme — {}", gen.name),
        GenerationKind::Layout => format!(" Generated layout — {}", gen.name),
    };

    let header_style = Style::default()
        .fg(theme.title_bar_fg)
        .bg(theme.title_bar_bg);

    let footer = " /generate accept | /generate reject ";
    let footer_style = Style::default()
        .fg(theme.status_bar_fg)
        .bg(theme.status_bar_bg);

    // Split area into header, content, footer
    let preview_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),   // header
            Constraint::Min(1),      // content
            Constraint::Length(1),   // footer
        ])
        .split(area);

    // Header
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(header, header_style))),
        preview_chunks[0],
    );

    // Content — show code with line numbers
    let content_lines: Vec<Line> = gen
        .content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            Line::from(vec![
                Span::styled(
                    format!("{:>3} ", i + 1),
                    Style::default().fg(theme.chat_timestamp),
                ),
                Span::styled(line.to_string(), Style::default().fg(theme.chat_message)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(content_lines);
    frame.render_widget(paragraph, preview_chunks[1]);

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, footer_style))),
        preview_chunks[2],
    );
}

/// Render a separator line between split panes.
fn render_separator(frame: &mut Frame, area: Rect, direction: SplitDirection, theme: &Theme) {
    let sep_style = Style::default().fg(theme.status_bar_fg).bg(theme.status_bar_bg);

    match direction {
        SplitDirection::Vertical => {
            // Vertical line
            let lines: Vec<Line> = (0..area.height)
                .map(|_| Line::from(Span::styled("│", sep_style)))
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        }
        SplitDirection::Horizontal => {
            // Horizontal line
            let bar = "─".repeat(area.width as usize);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(bar, sep_style))),
                area,
            );
        }
    }
}
