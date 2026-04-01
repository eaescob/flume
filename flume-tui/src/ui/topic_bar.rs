use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if app.viewing_global {
        let line = Line::from(Span::styled(
            " flume — global message buffer",
            Style::default().fg(theme.title_bar_fg),
        ));
        let bar = Paragraph::new(line).style(Style::default().bg(theme.title_bar_bg));
        frame.render_widget(bar, area);
        return;
    }

    let topic_text = app
        .active_server_state()
        .and_then(|ss| {
            let buf = ss.active_buf();
            buf.topic.as_deref()
        })
        .unwrap_or("");

    let display = if topic_text.is_empty() {
        match app.active_target() {
            Some(target) => format!(" {}", target),
            None => format!(" {}", app.active_server_name()),
        }
    } else {
        // Truncate topic to fit
        let max_len = area.width as usize;
        if topic_text.len() > max_len.saturating_sub(2) {
            format!(" {}...", &topic_text[..max_len.saturating_sub(5)])
        } else {
            format!(" {}", topic_text)
        }
    };

    let line = Line::from(Span::styled(
        display,
        Style::default().fg(theme.title_bar_fg),
    ));

    let bar = Paragraph::new(line).style(Style::default().bg(theme.title_bar_bg));
    frame.render_widget(bar, area);
}
