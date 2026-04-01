use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let conn_state = app.active_connection_state();
    let state_color = match conn_state {
        flume_core::event::ConnectionState::Connected => theme.state_connected,
        flume_core::event::ConnectionState::Connecting
        | flume_core::event::ConnectionState::Registering => theme.state_connecting,
        flume_core::event::ConnectionState::Disconnected => theme.state_disconnected,
    };

    let modes = app
        .active_server_state()
        .map(|s| s.user_modes.as_str())
        .unwrap_or("");
    let nick_display = if modes.is_empty() {
        format!(" [{}]", app.active_nick())
    } else {
        format!(" [{} {}]", app.active_nick(), modes)
    };

    let mut spans = vec![
        Span::styled(nick_display, Style::default().fg(theme.status_bar_fg)),
        Span::raw(" | "),
        Span::styled(
            format!("{}", conn_state),
            Style::default().fg(state_color),
        ),
        Span::raw(" | "),
        Span::styled(
            app.active_server_name().to_string(),
            Style::default().fg(theme.status_bar_fg),
        ),
    ];

    // Show other servers with unread indicators
    for name in &app.server_order {
        if Some(name.as_str()) == app.active_server.as_deref() {
            continue;
        }
        if let Some(ss) = app.servers.get(name) {
            let unread = ss.total_unread();
            let highlights = ss.total_highlights();
            let state_indicator = match ss.connection_state {
                flume_core::event::ConnectionState::Connected => "",
                flume_core::event::ConnectionState::Disconnected => "✗",
                _ => "…",
            };
            spans.push(Span::raw(" | "));
            if highlights > 0 {
                spans.push(Span::styled(
                    format!("{}{}({}!)", name, state_indicator, unread),
                    Style::default().fg(theme.chat_highlight),
                ));
            } else if unread > 0 {
                spans.push(Span::styled(
                    format!("{}{}({})", name, state_indicator, unread),
                    Style::default().fg(theme.unread),
                ));
            } else {
                spans.push(Span::styled(
                    format!("{}{}", name, state_indicator),
                    Style::default().fg(theme.inactive),
                ));
            }
        }
    }

    // Show active DCC transfers
    for t in &app.dcc_transfers {
        if let flume_core::dcc::DccTransferState::Active { bytes_transferred, total } = &t.state {
            let name = t.offer.filename.as_deref().unwrap_or("chat");
            let pct = if *total > 0 {
                format!("{}%", (*bytes_transferred * 100) / total)
            } else {
                flume_core::dcc::format_size(*bytes_transferred)
            };
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("[DCC {} {}]", name, pct),
                Style::default().fg(theme.unread),
            ));
        }
    }

    // Show generation status
    if app.generating {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "[generating...]",
            Style::default().fg(theme.unread),
        ));
    }

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.status_bar_bg));
    frame.render_widget(bar, area);
}
