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

    // Buffer list — sorted alphabetically with group entries
    let sorted_buffers = ss.sorted_buffers_with_groups(&app.groups, app.active_group.as_deref());

    for (visual_idx, buf_name) in sorted_buffers.iter().enumerate() {
        let idx = visual_idx + 1;
        let is_group = buf_name.starts_with('[') && buf_name.ends_with(']');
        let display = if buf_name.is_empty() {
            "server"
        } else {
            buf_name.as_str()
        };

        let is_active = if is_group {
            let group_name = &buf_name[1..buf_name.len()-1];
            app.active_group.as_deref() == Some(group_name)
        } else {
            *buf_name == ss.active_buffer && app.active_group.is_none()
        };

        // For groups, aggregate unread from both member channels
        let (unread, highlights) = if is_group {
            let group_name = &buf_name[1..buf_name.len()-1];
            if let Some(g) = app.groups.get(group_name) {
                let u: u32 = g.channels.iter()
                    .filter_map(|c| ss.buffers.get(c.as_str()))
                    .map(|b| b.unread_count)
                    .sum();
                let h: u32 = g.channels.iter()
                    .filter_map(|c| ss.buffers.get(c.as_str()))
                    .map(|b| b.highlight_count)
                    .sum();
                (u, h)
            } else {
                (0, 0)
            }
        } else {
            let buf = ss.buffers.get(buf_name.as_str());
            (buf.map(|b| b.unread_count).unwrap_or(0), buf.map(|b| b.highlight_count).unwrap_or(0))
        };

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
