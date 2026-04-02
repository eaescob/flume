use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(ss) = app.active_server_state() else {
        let paragraph = Paragraph::new("").style(Style::default().bg(theme.buffer_list_bg));
        frame.render_widget(paragraph, area);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Global flume buffer — always first
    if app.viewing_global {
        lines.push(Line::from(Span::styled(
            " flume",
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " flume",
            Style::default().fg(theme.buffer_list_fg),
        )));
    }

    // Server header
    lines.push(Line::from(Span::styled(
        format!(" {}", app.active_server_name()),
        Style::default()
            .fg(theme.title_bar_fg)
            .add_modifier(Modifier::BOLD),
    )));

    // Buffer list — sorted alphabetically, server buffer first
    let sorted_buffers = ss.sorted_buffers();

    for (visual_idx, buf_name) in sorted_buffers.iter().enumerate() {
        let idx = visual_idx + 1; // 1-indexed display number
        let display = if buf_name.is_empty() {
            "server"
        } else {
            buf_name.as_str()
        };
        let is_active = *buf_name == ss.active_buffer;
        let buf = ss.buffers.get(buf_name.as_str());
        let unread = buf.map(|b| b.unread_count).unwrap_or(0);
        let highlights = buf.map(|b| b.highlight_count).unwrap_or(0);

        let (label, style) = if is_active {
            (
                format!(" {}.{}", idx, display),
                Style::default()
                    .fg(theme.active)
                    .add_modifier(Modifier::BOLD),
            )
        } else if highlights > 0 {
            (
                format!(" {}.{}({}!)", idx, display, unread),
                Style::default().fg(theme.chat_highlight),
            )
        } else if unread > 0 {
            (
                format!(" {}.{}({})", idx, display, unread),
                Style::default().fg(theme.unread),
            )
        } else {
            (
                format!(" {}.{}", idx, display),
                Style::default().fg(theme.buffer_list_fg),
            )
        };

        lines.push(Line::from(Span::styled(label, style)));
    }

    // Pad to fill height
    while lines.len() < area.height as usize {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).style(Style::default().bg(theme.buffer_list_bg));
    frame.render_widget(paragraph, area);
}
