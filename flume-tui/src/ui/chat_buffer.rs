use std::collections::VecDeque;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use flume_core::irc_format::{self, FormattedSpan};

use crate::app::{App, DisplayMessage, MessageSource};
use crate::theme::Theme;

/// mIRC color palette (0-15) mapped to ratatui colors.
fn irc_color(code: u8) -> Color {
    match code {
        0 => Color::White,
        1 => Color::Black,
        2 => Color::Blue,
        3 => Color::Green,
        4 => Color::Red,
        5 => Color::Indexed(52),
        6 => Color::Magenta,
        7 => Color::Indexed(208),
        8 => Color::Yellow,
        9 => Color::LightGreen,
        10 => Color::Cyan,
        11 => Color::LightCyan,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::DarkGray,
        15 => Color::Gray,
        // Extended colors 16-98: map to terminal 256-color palette
        16..=98 => Color::Indexed(code - 16 + 16),
        _ => Color::Reset,
    }
}

/// Convert an IRC FormattedSpan to a ratatui Span.
fn irc_span_to_ratatui(span: &FormattedSpan, base_style: Style) -> Span<'static> {
    let mut style = base_style;
    if let Some(fg_code) = span.fg { style = style.fg(irc_color(fg_code)); }
    if let Some(bg_code) = span.bg { style = style.bg(irc_color(bg_code)); }
    if span.reverse {
        let fg = style.fg.unwrap_or(Color::Reset);
        let bg = style.bg.unwrap_or(Color::Reset);
        style = style.fg(bg).bg(fg);
    }
    let mut mods = Modifier::empty();
    if span.bold { mods |= Modifier::BOLD; }
    if span.italic { mods |= Modifier::ITALIC; }
    if span.underline { mods |= Modifier::UNDERLINED; }
    style = style.add_modifier(mods);
    Span::styled(span.text.clone(), style)
}
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
    let total = messages.len();

    // Take all messages from scroll_offset to the end and format them.
    // We take more than the visible height because wrapped lines take
    // extra vertical space. Paragraph::scroll handles showing the bottom.
    let end = total.saturating_sub(scroll_offset);
    // Take up to 200 recent messages (more than any screen height, accounts for wrapping)
    let start = end.saturating_sub(200);

    let lines: Vec<Line> = messages
        .iter()
        .skip(start)
        .take(end - start)
        .map(|msg| format_message(msg, timestamp_format, search, theme))
        .collect();

    let height = area.height as usize;
    let paragraph = Paragraph::new(lines.clone())
        .wrap(ratatui::widgets::Wrap { trim: false });

    // Calculate how many visual lines the paragraph produces, then scroll
    // to the bottom so the latest messages are visible.
    // Approximate: count lines + extra for wrapping (assume avg 1.5x for wrapped)
    let visual_lines: usize = lines.iter()
        .map(|l| {
            let width = area.width as usize;
            if width == 0 { return 1; }
            let line_width: usize = l.spans.iter().map(|s| s.content.len()).sum();
            (line_width / width.max(1)) + 1
        })
        .sum();
    let scroll_y = visual_lines.saturating_sub(height) as u16;

    let paragraph = paragraph.scroll((scroll_y, 0));
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

    // Strip IRC formatting for URL detection (URLs can span format boundaries)
    let plain = irc_format::strip_formatting(text);
    let url_spans = url::find_urls(&plain);

    if url_spans.is_empty() {
        // No URLs — render with IRC formatting
        return irc_format::parse_irc_format(text)
            .iter()
            .map(|s| irc_span_to_ratatui(s, effective_base))
            .collect();
    }

    // Has URLs — for simplicity, strip IRC formatting and render with URL highlighting
    // (mixing IRC colors + URL detection is complex; this preserves URL functionality)
    let mut spans = Vec::new();
    let mut pos = 0;
    for (start, end) in &url_spans {
        if pos < *start {
            if let Some(segment) = plain.get(pos..*start) {
                spans.push(Span::styled(segment.to_string(), effective_base));
            }
        }
        if let Some(segment) = plain.get(*start..*end) {
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
