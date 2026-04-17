use std::collections::VecDeque;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use flume_core::irc_format::{self, FormattedSpan};
use crate::app::ChannelNick;

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
    let nicks = app.active_nicks();
    let search = app
        .active_server_state()
        .and_then(|ss| ss.active_buf().search.as_deref());

    render_buffer(frame, area, messages, scroll_offset, search, &app.timestamp_format, nicks, theme);
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
    nicks: &[ChannelNick],
    theme: &Theme,
) {
    let total = messages.len();

    // Take all messages from scroll_offset to the end and format them.
    // We take more than the visible height because wrapped lines take
    // extra vertical space. Paragraph::scroll handles showing the bottom.
    let end = total.saturating_sub(scroll_offset);
    // Take up to 200 recent messages (more than any screen height, accounts for wrapping)
    let start = end.saturating_sub(200);

    let width = area.width as usize;
    let height = area.height as usize;

    // Build visual lines: wrap normal text at width, keep art lines intact.
    // Art is detected by the presence of background color codes (\x03FG,BG).
    let mut visual_lines: Vec<Line> = Vec::new();
    for msg in messages.iter().skip(start).take(end - start) {
        let line = format_message(msg, timestamp_format, search, nicks, theme);
        let is_art = has_background_colors(&msg.text);

        if is_art || width == 0 {
            visual_lines.push(line);
        } else {
            let line_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
            if line_width <= width {
                visual_lines.push(line);
            } else {
                // Soft-wrap: split into multiple visual lines
                let wrapped = wrap_line(&line, width);
                visual_lines.extend(wrapped);
            }
        }
    }

    // Show the last `height` visual lines (scroll to bottom)
    let skip = visual_lines.len().saturating_sub(height);
    let visible: Vec<Line> = visual_lines.into_iter().skip(skip).take(height).collect();

    let paragraph = Paragraph::new(visible);
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

/// Check if text contains background color codes (likely ASCII art).
fn has_background_colors(text: &str) -> bool {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x03 {
            i += 1;
            // Skip FG digits
            let start = i;
            while i < len && i - start < 2 && bytes[i].is_ascii_digit() { i += 1; }
            // If comma follows, there's a background color
            if i < len && bytes[i] == b',' {
                return true;
            }
        } else if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            // ANSI escape: check for background color (40-47, 49, 100-107)
            i += 2;
            let param_start = i;
            while i < len && bytes[i] != b'm' && bytes[i].is_ascii_graphic() { i += 1; }
            if i < len && bytes[i] == b'm' {
                let params = std::str::from_utf8(&bytes[param_start..i]).unwrap_or("");
                for p in params.split(';') {
                    if let Ok(n) = p.parse::<u16>() {
                        if (40..=47).contains(&n) || (100..=107).contains(&n) {
                            return true;
                        }
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Wrap a Line into multiple visual lines at the given width.
fn wrap_line<'a>(line: &Line<'a>, width: usize) -> Vec<Line<'a>> {
    if width == 0 {
        return vec![line.clone()];
    }

    // Flatten all spans into one string, tracking style boundaries
    let mut chars: Vec<(char, Style)> = Vec::new();
    for span in &line.spans {
        for ch in span.content.chars() {
            chars.push((ch, span.style));
        }
    }

    let mut result = Vec::new();
    let mut pos = 0;
    while pos < chars.len() {
        let end = (pos + width).min(chars.len());

        // Try to break at a space for word-wrapping
        let mut break_at = end;
        if end < chars.len() {
            // Look back for a space
            let mut j = end;
            while j > pos + width / 2 {
                if chars[j].0 == ' ' {
                    break_at = j + 1;
                    break;
                }
                j -= 1;
            }
        }

        // Build spans for this visual line
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut current_text = String::new();
        let mut current_style = if pos < chars.len() { chars[pos].1 } else { Style::default() };

        for &(ch, style) in &chars[pos..break_at] {
            if style != current_style {
                if !current_text.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut current_text), current_style));
                }
                current_style = style;
            }
            current_text.push(ch);
        }
        if !current_text.is_empty() {
            spans.push(Span::styled(current_text, current_style));
        }

        result.push(Line::from(spans));
        pos = break_at;
    }

    if result.is_empty() {
        result.push(line.clone());
    }
    result
}

/// Return the highest-priority prefix symbol for a nick.
/// Priority: ~ (owner) > & (admin) > @ (op) > % (halfop) > + (voice)
fn nick_prefix(nick: &str, nicks: &[ChannelNick]) -> &'static str {
    const ORDER: &[(char, &str)] = &[
        ('~', "~"),
        ('&', "&"),
        ('@', "@"),
        ('%', "%"),
        ('+', "+"),
    ];
    nicks
        .iter()
        .find(|cn| cn.nick == nick)
        .and_then(|cn| {
            ORDER.iter().find(|(c, _)| cn.prefix.contains(*c)).map(|(_, s)| *s)
        })
        .unwrap_or("")
}

fn format_message<'a>(
    msg: &DisplayMessage,
    timestamp_format: &str,
    search: Option<&str>,
    nicks: &[ChannelNick],
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
            let prefix = nick_prefix(nick, nicks);
            let nick_color = theme.nick_color(nick);
            let base = Style::default().fg(theme.chat_message);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
                Span::styled(format!("<{}{}> ", prefix, nick), Style::default().fg(nick_color)),
            ];
            spans.extend(styled_text_spans(&msg.text, base, url_style, msg.highlight, highlight_style, search_style));
            Line::from(spans)
        }
        MessageSource::Own(nick) => {
            let prefix = nick_prefix(nick, nicks);
            let base = Style::default().fg(theme.chat_message);
            let mut spans = vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(theme.chat_timestamp)),
                Span::styled(format!("<{}{}> ", prefix, nick), Style::default().fg(theme.chat_own_nick)),
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
