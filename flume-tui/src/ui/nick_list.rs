use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let nicks = match app.active_server_state() {
        Some(ss) => &ss.active_buf().nicks,
        None => return,
    };

    if nicks.is_empty() {
        return;
    }

    let height = area.height as usize;
    let lines: Vec<Line> = nicks
        .iter()
        .take(height)
        .map(|cn| {
            let color = if cn.prefix.contains('@') {
                theme.nick_list_op
            } else if cn.prefix.contains('+') {
                theme.nick_list_voice
            } else {
                theme.nick_list_fg
            };
            Line::from(Span::styled(
                format!("{}{}", cn.prefix, cn.nick),
                Style::default().fg(color),
            ))
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
