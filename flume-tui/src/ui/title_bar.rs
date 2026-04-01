use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut spans = vec![Span::styled(
        format!(" [{}]", app.active_server_name()),
        Style::default().fg(theme.title_bar_fg),
    )];

    if let Some(target) = app.active_target() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            target.to_string(),
            Style::default().fg(theme.active),
        ));
    }

    // Show buffer list for active server
    if let Some(ss) = app.active_server_state() {
        if ss.buffer_order.len() > 1 {
            spans.push(Span::raw("  "));
            for (i, buf_name) in ss.buffer_order.iter().enumerate() {
                let display = if buf_name.is_empty() {
                    "server".to_string()
                } else {
                    buf_name.clone()
                };
                let is_active = *buf_name == ss.active_buffer;
                let buf = ss.buffers.get(buf_name);
                let unread = buf.map(|b| b.unread_count).unwrap_or(0);
                let highlights = buf.map(|b| b.highlight_count).unwrap_or(0);

                if is_active {
                    spans.push(Span::styled(
                        format!("[{}]", display),
                        Style::default().fg(theme.active),
                    ));
                } else if highlights > 0 {
                    spans.push(Span::styled(
                        format!("{}({}!)", display, unread),
                        Style::default().fg(theme.chat_highlight),
                    ));
                } else if unread > 0 {
                    spans.push(Span::styled(
                        format!("{}({})", display, unread),
                        Style::default().fg(theme.unread),
                    ));
                } else {
                    spans.push(Span::styled(
                        display,
                        Style::default().fg(theme.inactive),
                    ));
                }
                if i + 1 < ss.buffer_order.len() {
                    spans.push(Span::raw(" "));
                }
            }
        }
    }

    // Split indicator
    if let Some(ref split) = app.split {
        let dir_char = match split.direction {
            crate::split::SplitDirection::Vertical => "│",
            crate::split::SplitDirection::Horizontal => "─",
        };
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("[{} {}]", dir_char, split.secondary_buffer),
            Style::default().fg(theme.inactive),
        ));
    }

    let title = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.title_bar_bg));
    frame.render_widget(title, area);
}
