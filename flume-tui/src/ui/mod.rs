pub mod buffer_list;
pub mod chat_buffer;
pub mod input_line;
pub mod nick_list;
pub mod splash;
pub mod status_bar;
pub mod title_bar;
pub mod topic_bar;

use std::collections::VecDeque;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, GenerationKind};
use crate::split::SplitDirection;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, app: &mut App, theme: &Theme) {
    // Layout:
    //   [buffer_list (full height) | center area | nick_list (full height)]
    //   status_bar (1 line, full width)
    //   input_line (1 line, full width)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // main area (buffer list + center + nick list)
            Constraint::Length(1), // status bar
            Constraint::Length(1), // input line
        ])
        .split(frame.area());

    // Show splash screen until first input or server connection
    if app.show_splash && app.servers.is_empty() {
        splash::render(frame, outer[0], theme);
        status_bar::render(frame, outer[1], app, theme);
        input_line::render(frame, outer[2], app, theme);
        return;
    }

    let show_buffer_list = app.active_server_state().is_some();
    let show_nick_list = app
        .active_server_state()
        .map(|ss| ss.active_buffer.starts_with('#') && !ss.active_buf().nicks.is_empty())
        .unwrap_or(false);

    // Build horizontal columns for main area
    let mut col_constraints = Vec::new();
    if show_buffer_list {
        col_constraints.push(Constraint::Length(20));
    }
    col_constraints.push(Constraint::Min(1)); // center (topic + chat)
    if show_nick_list && app.pending_generation.is_none() {
        col_constraints.push(Constraint::Length(18));
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints)
        .split(outer[0]);

    let mut col_idx = 0;

    // Buffer list (left column, full height)
    if show_buffer_list {
        app.buffer_list_area = columns[col_idx];
        buffer_list::render(frame, columns[col_idx], app, theme);
        col_idx += 1;
    }

    let center_area = columns[col_idx];
    col_idx += 1;

    // Nick list (right column, full height)
    if show_nick_list && app.pending_generation.is_none() && app.split.is_none() {
        nick_list::render(frame, columns[col_idx], app, theme);
    }

    // Center area: topic bar (1 line) + chat/split/preview
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // topic bar
            Constraint::Min(1),   // chat area
        ])
        .split(center_area);

    topic_bar::render(frame, center[0], app, theme);
    app.chat_area = center[1];

    if let Some(ref gen) = app.pending_generation {
        // Generation preview: chat | separator | preview
        let preview_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(center[1]);

        chat_buffer::render(frame, preview_chunks[0], app, theme);
        render_separator(frame, preview_chunks[1], SplitDirection::Vertical, theme);
        render_generation_preview(frame, preview_chunks[2], gen, theme);
    } else if let Some(ref split) = app.split {
        // Split mode: chat1 | separator | chat2
        let direction = match split.direction {
            SplitDirection::Vertical => Direction::Horizontal,
            SplitDirection::Horizontal => Direction::Vertical,
        };

        let split_chunks = Layout::default()
            .direction(direction)
            .constraints([
                Constraint::Percentage(split.ratio),
                Constraint::Length(1),
                Constraint::Percentage(100 - split.ratio),
            ])
            .split(center[1]);

        app.primary_pane_area = split_chunks[0];
        app.secondary_pane_area = split_chunks[2];
        chat_buffer::render(frame, split_chunks[0], app, theme);
        // Show secondary channel name in the separator
        let sep_label = Some(split.secondary_buffer.as_str());
        render_separator_labeled(frame, split_chunks[1], split.direction, theme, sep_label);

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
        // Normal: just chat
        chat_buffer::render(frame, center[1], app, theme);
    }

    // Status bar and input (full width)
    status_bar::render(frame, outer[1], app, theme);
    input_line::render(frame, outer[2], app, theme);
}

/// Render a separator line between split panes, optionally with a label.
fn render_separator(
    frame: &mut Frame,
    area: Rect,
    direction: SplitDirection,
    theme: &Theme,
) {
    render_separator_labeled(frame, area, direction, theme, None);
}

/// Render a labeled separator between split panes.
fn render_separator_labeled(
    frame: &mut Frame,
    area: Rect,
    direction: SplitDirection,
    theme: &Theme,
    label: Option<&str>,
) {
    let sep_style = Style::default().fg(theme.status_bar_fg).bg(theme.status_bar_bg);
    let label_style = Style::default()
        .fg(theme.active)
        .bg(theme.status_bar_bg)
        .add_modifier(Modifier::BOLD);

    match direction {
        SplitDirection::Vertical => {
            let lines: Vec<Line> = (0..area.height)
                .map(|_| Line::from(Span::styled("│", sep_style)))
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        }
        SplitDirection::Horizontal => {
            let width = area.width as usize;
            let line = if let Some(name) = label {
                // ── channel-name ──────────
                let prefix = "── ";
                let suffix_char = '─';
                let label_len = prefix.len() + name.len() + 1; // +1 for space after
                let remaining = width.saturating_sub(label_len);
                let suffix: String = std::iter::repeat(suffix_char).take(remaining).collect();
                Line::from(vec![
                    Span::styled(prefix.to_string(), sep_style),
                    Span::styled(name.to_string(), label_style),
                    Span::styled(format!(" {}", suffix), sep_style),
                ])
            } else {
                let bar = "─".repeat(width);
                Line::from(Span::styled(bar, sep_style))
            };
            frame.render_widget(Paragraph::new(line), area);
        }
    }
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

    let preview_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(header, header_style))),
        preview_chunks[0],
    );

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

    frame.render_widget(Paragraph::new(content_lines), preview_chunks[1]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, footer_style))),
        preview_chunks[2],
    );
}
