use std::collections::VecDeque;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, DisplayMessage, MessageSource};
use crate::theme::Theme;
use crate::url;

/// Render the active buffer's messages into the given area.
pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let messages = app.active_messages();
    let scroll_offset = app.active_scroll_offset();
    let search = app
        .active_server_state()
        .and_then(|ss| ss.active_buf().search.as_deref());

    render_buffer(frame, area, messages, scroll_offset, search, &app.timestamp_format, theme);
}

/// Render any buffer's messages into the given area.
/// Used by both the primary and split pane renderers.
pub fn render_buffer(
    frame: &mut Frame,
    area: Rect,
    messages: &VecDeque<DisplayMessage>,
    scroll_offset: usize,
    search: Option<&str>,
    timestamp_format: &str,
    theme: &Theme,
) {
    let height = area.height as usize;
    let total = messages.len();

    let end = total.saturating_sub(scroll_offset);
    let start = end.saturating_sub(height);

    let lines: Vec<Line> = messages
        .iter()
        .skip(start)
        .take(end - start)
        .map(|msg| format_message(msg, timestamp_format, search, theme))
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);

    if scroll_offset > 0 {
        let indicator = Span::styled(
            format!("-- {} more below --", scroll_offset),
            Style::default().fg(theme.scroll_indicator),
        );
        let indicator_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(indicator)), indicator_area);
    }
}

/// Split message text into styled spans, applying URL coloring and highlight styling.
fn styled_text_spans(
    text: &str,
    base_style: Style,
    url_style: Style,
    is_highlight: bool,
    highlight_style: Style,
    search_style: Option<Style>,
) -> Vec<Span<'static>> {
    // Search match overrides everything
    if let Some(search) = search_style {
        return vec![Span::styled(text.to_string(), search)];
    }

    let effective_base = if is_highlight { highlight_style } else { base_style };
    let effective_url = url_style.add_modifier(Modifier::UNDERLINED);

    let url_spans = url::find_urls(text);
    if url_spans.is_empty() {
        return vec![Span::styled(text.to_string(), effective_base)];
    }

    let mut spans = Vec::new();
    let mut pos = 0;
    for (start, end) in &url_spans {
        if pos < *start {
            if let Some(segment) = text.get(pos..*start) {
                spans.push(Span::styled(segment.to_string(), effective_base));
            }
        }
        if let Some(segment) = text.get(*start..*end) {
            spans.push(Span::styled(segment.to_string(), effective_url));
        }
        pos = *end;
    }
    if pos < text.len() {
        if let Some(segment) = text.get(pos..) {
            spans.push(Span::styled(segment.to_string(), effective_base));
        }
    }
    spans
}

fn format_message<'a>(
    msg: &DisplayMessage,
    timestamp_format: &str,
    search: Option<&str>,
    theme: &Theme,
) -> Line<'a> {
    let ts = msg.timestamp.format(timestamp_format).to_string();
    let is_search_match = search
        .map(|s| msg.text.to_lowercase().contains(s))
        .unwrap_or(false);
    let search_style = if is_search_match {
        Some(Style::default().bg(theme.search_match_bg).fg(theme.search_match_fg))
    } else {
        None
    };

    let highlight_style = Style::default().fg(theme.chat_highlight);
    let url_style = Style::default().fg(theme.chat_url);

    match &msg.source {
        MessageSource::User(nick) => {
            let nick_color = theme.nick_color(nick);
            let base = Style::default().fg(theme.chat_message);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
                Span::styled(format!("<{}> ", nick), Style::default().fg(nick_color)),
            ];
            spans.extend(styled_text_spans(&msg.text, base, url_style, msg.highlight, highlight_style, search_style));
            Line::from(spans)
        }
        MessageSource::Own(nick) => {
            let base = Style::default().fg(theme.chat_message);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
                Span::styled(format!("<{}> ", nick), Style::default().fg(theme.chat_own_nick)),
            ];
            spans.extend(styled_text_spans(&msg.text, base, url_style, false, highlight_style, search_style));
            Line::from(spans)
        }
        MessageSource::Action(nick) => {
            let base = Style::default().fg(theme.chat_action);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
                Span::styled(format!("* {} ", nick), Style::default().fg(theme.chat_action)),
            ];
            spans.extend(styled_text_spans(&msg.text, base, url_style, msg.highlight, highlight_style, search_style));
            Line::from(spans)
        }
        MessageSource::Server => {
            let base = Style::default().fg(theme.chat_server);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
            ];
            spans.extend(styled_text_spans(&msg.text, base, url_style, false, highlight_style, search_style));
            Line::from(spans)
        }
        MessageSource::System => {
            let base = Style::default().fg(theme.chat_system);
            let text = format!("-- {} --", msg.text);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
            ];
            spans.extend(styled_text_spans(&text, base, url_style, false, highlight_style, search_style));
            Line::from(spans)
        }
    }
}
