use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::Theme;

const LOGO: &[&str] = &[
    r"  _____ _                        ",
    r" |  ___| |_   _ _ __ ___   ___  ",
    r" | |_  | | | | | '_ ` _ \ / _ \ ",
    r" |  _| | | |_| | | | | | |  __/ ",
    r" |_|   |_|\__,_|_| |_| |_|\___| ",
];

const TAGLINE: &str = "Modern IRC for the terminal";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(frame: &mut Frame, area: Rect, theme: &Theme) {
    let logo_height = LOGO.len();
    let total_height = logo_height + 4; // logo + blank + tagline + version + blank
    let start_y = if area.height as usize > total_height + 2 {
        (area.height as usize - total_height) / 2
    } else {
        1
    };

    let logo_width = LOGO.iter().map(|l| l.len()).max().unwrap_or(0);
    let center_x = if area.width as usize > logo_width {
        (area.width as usize - logo_width) / 2
    } else {
        0
    };

    let tagline_x = if area.width as usize > TAGLINE.len() {
        (area.width as usize - TAGLINE.len()) / 2
    } else {
        0
    };

    let version_str = format!("v{}", VERSION);
    let version_x = if area.width as usize > version_str.len() {
        (area.width as usize - version_str.len()) / 2
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();

    // Pad to center vertically
    for _ in 0..start_y {
        lines.push(Line::from(""));
    }

    // Logo
    for logo_line in LOGO {
        let pad = " ".repeat(center_x);
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad, logo_line),
            Style::default()
                .fg(theme.active)
                .add_modifier(Modifier::BOLD),
        )));
    }

    // Blank line
    lines.push(Line::from(""));

    // Tagline
    let pad = " ".repeat(tagline_x);
    lines.push(Line::from(Span::styled(
        format!("{}{}", pad, TAGLINE),
        Style::default().fg(theme.status_bar_fg),
    )));

    // Version
    let pad = " ".repeat(version_x);
    lines.push(Line::from(Span::styled(
        format!("{}{}", pad, version_str),
        Style::default()
            .fg(theme.inactive)
            .add_modifier(Modifier::DIM),
    )));

    // Pad remaining
    while lines.len() < area.height as usize {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
