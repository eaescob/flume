//! IRC formatting code parser and generator.
//!
//! Handles mIRC color codes and formatting:
//! - \x02 Bold
//! - \x1d Italic
//! - \x1f Underline
//! - \x16 Reverse (swap fg/bg)
//! - \x0f Reset all formatting
//! - \x03FG[,BG] Color (0-15)
//!
//! For user input, we support %B %I %U %R %O and %C<fg>[,<bg>] shortcuts.
//! Scripts use flume.format.bold(), flume.format.color(), etc.

/// A segment of text with IRC formatting applied.
#[derive(Debug, Clone)]
pub struct FormattedSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub fg: Option<u8>,
    pub bg: Option<u8>,
}

/// Parse IRC-formatted text into styled spans.
pub fn parse_irc_format(text: &str) -> Vec<FormattedSpan> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut reverse = false;
    let mut fg: Option<u8> = None;
    let mut bg: Option<u8> = None;

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            0x02 => {
                // Bold toggle
                if !current.is_empty() {
                    spans.push(FormattedSpan {
                        text: std::mem::take(&mut current),
                        bold, italic, underline, reverse, fg, bg,
                    });
                }
                bold = !bold;
                i += 1;
            }
            0x1d => {
                // Italic toggle
                if !current.is_empty() {
                    spans.push(FormattedSpan {
                        text: std::mem::take(&mut current),
                        bold, italic, underline, reverse, fg, bg,
                    });
                }
                italic = !italic;
                i += 1;
            }
            0x1f => {
                // Underline toggle
                if !current.is_empty() {
                    spans.push(FormattedSpan {
                        text: std::mem::take(&mut current),
                        bold, italic, underline, reverse, fg, bg,
                    });
                }
                underline = !underline;
                i += 1;
            }
            0x16 => {
                // Reverse toggle
                if !current.is_empty() {
                    spans.push(FormattedSpan {
                        text: std::mem::take(&mut current),
                        bold, italic, underline, reverse, fg, bg,
                    });
                }
                reverse = !reverse;
                i += 1;
            }
            0x0f => {
                // Reset all
                if !current.is_empty() {
                    spans.push(FormattedSpan {
                        text: std::mem::take(&mut current),
                        bold, italic, underline, reverse, fg, bg,
                    });
                }
                bold = false;
                italic = false;
                underline = false;
                reverse = false;
                fg = None;
                bg = None;
                i += 1;
            }
            0x03 => {
                // Color: \x03FG[,BG]
                if !current.is_empty() {
                    spans.push(FormattedSpan {
                        text: std::mem::take(&mut current),
                        bold, italic, underline, reverse, fg, bg,
                    });
                }
                i += 1;
                // Parse FG (1-2 digits)
                let fg_start = i;
                while i < len && i - fg_start < 2 && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > fg_start {
                    let fg_str = std::str::from_utf8(&bytes[fg_start..i]).unwrap_or("0");
                    fg = fg_str.parse::<u8>().ok().map(|n| n.min(98));
                } else {
                    // \x03 with no digits = reset colors
                    fg = None;
                    bg = None;
                }
                // Parse BG after comma
                if i < len && bytes[i] == b',' {
                    i += 1;
                    let bg_start = i;
                    while i < len && i - bg_start < 2 && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i > bg_start {
                        let bg_str = std::str::from_utf8(&bytes[bg_start..i]).unwrap_or("0");
                        bg = bg_str.parse::<u8>().ok().map(|n| n.min(98));
                    }
                }
            }
            _ => {
                current.push(bytes[i] as char);
                i += 1;
            }
        }
    }

    if !current.is_empty() {
        spans.push(FormattedSpan {
            text: current,
            bold, italic, underline, reverse, fg, bg,
        });
    }

    spans
}

/// Strip all IRC formatting codes from text, returning plain text.
pub fn strip_formatting(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            0x02 | 0x1d | 0x1f | 0x16 | 0x0f => { i += 1; }
            0x03 => {
                i += 1;
                // Skip FG digits
                let start = i;
                while i < len && i - start < 2 && bytes[i].is_ascii_digit() { i += 1; }
                // Skip ,BG digits
                if i < len && bytes[i] == b',' {
                    i += 1;
                    let start = i;
                    while i < len && i - start < 2 && bytes[i].is_ascii_digit() { i += 1; }
                }
            }
            b => {
                result.push(b as char);
                i += 1;
            }
        }
    }
    result
}

/// Convert user-friendly format shortcuts to IRC control codes.
/// %B = bold, %I = italic, %U = underline, %R = reverse, %O = reset
/// %C<fg>[,<bg>] = color (e.g., %C4 = red, %C4,1 = red on black)
pub fn apply_input_shortcuts(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '%' && i + 1 < len {
            match chars[i + 1] {
                'B' | 'b' => { result.push('\x02'); i += 2; }
                'I' | 'i' => { result.push('\x1d'); i += 2; }
                'U' | 'u' => { result.push('\x1f'); i += 2; }
                'R' | 'r' => { result.push('\x16'); i += 2; }
                'O' | 'o' => { result.push('\x0f'); i += 2; }
                'C' | 'c' => {
                    result.push('\x03');
                    i += 2;
                    // Copy through digits and optional comma+digits
                    while i < len && chars[i].is_ascii_digit() { result.push(chars[i]); i += 1; }
                    if i < len && chars[i] == ',' {
                        result.push(',');
                        i += 1;
                        while i < len && chars[i].is_ascii_digit() { result.push(chars[i]); i += 1; }
                    }
                }
                '%' => { result.push('%'); i += 2; } // escaped %%
                _ => { result.push('%'); i += 1; } // not a shortcut
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bold() {
        let spans = parse_irc_format("\x02hello\x02 world");
        assert_eq!(spans.len(), 2);
        assert!(spans[0].bold);
        assert_eq!(spans[0].text, "hello");
        assert!(!spans[1].bold);
        assert_eq!(spans[1].text, " world");
    }

    #[test]
    fn parse_color() {
        let spans = parse_irc_format("\x034red text\x03 normal");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].fg, Some(4));
        assert_eq!(spans[0].text, "red text");
        assert_eq!(spans[1].fg, None);
    }

    #[test]
    fn parse_color_with_bg() {
        let spans = parse_irc_format("\x034,1red on black\x03");
        assert_eq!(spans[0].fg, Some(4));
        assert_eq!(spans[0].bg, Some(1));
    }

    #[test]
    fn parse_reset() {
        let spans = parse_irc_format("\x02\x034bold red\x0f normal");
        assert!(spans[0].bold);
        assert_eq!(spans[0].fg, Some(4));
        assert!(!spans[1].bold);
        assert_eq!(spans[1].fg, None);
    }

    #[test]
    fn strip() {
        assert_eq!(strip_formatting("\x02bold\x02 \x034,1colored\x03 text"), "bold colored text");
    }

    #[test]
    fn input_shortcuts() {
        assert_eq!(apply_input_shortcuts("%Bbold%B"), "\x02bold\x02");
        assert_eq!(apply_input_shortcuts("%C4red%O"), "\x034red\x0f");
        assert_eq!(apply_input_shortcuts("%C4,1red on black"), "\x034,1red on black");
        assert_eq!(apply_input_shortcuts("100%%"), "100%");
    }

    #[test]
    fn plain_text_unchanged() {
        let spans = parse_irc_format("hello world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert!(!spans[0].bold);
        assert_eq!(spans[0].fg, None);
    }
}
