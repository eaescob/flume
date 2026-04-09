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
            0x1b => {
                // ANSI escape: ESC[ params m
                if i + 1 < len && bytes[i + 1] == b'[' {
                    if !current.is_empty() {
                        spans.push(FormattedSpan {
                            text: std::mem::take(&mut current),
                            bold, italic, underline, reverse, fg, bg,
                        });
                    }
                    i += 2; // skip ESC[
                    // Collect params until 'm'
                    let param_start = i;
                    while i < len && bytes[i] != b'm' && bytes[i] != b'H' && bytes[i] != b'J' && bytes[i] != b'K' {
                        i += 1;
                    }
                    if i < len && bytes[i] == b'm' {
                        let params_str = std::str::from_utf8(&bytes[param_start..i]).unwrap_or("");
                        let params: Vec<u8> = params_str
                            .split(';')
                            .filter_map(|s| s.parse::<u8>().ok())
                            .collect();
                        apply_ansi_params(&params, &mut bold, &mut italic, &mut underline, &mut reverse, &mut fg, &mut bg);
                        i += 1; // skip 'm'
                    } else {
                        // Non-SGR escape — skip the terminator
                        if i < len { i += 1; }
                    }
                } else {
                    // Non-CSI ESC — push as UTF-8
                    let ch_len = utf8_char_len(bytes[i]);
                    if i + ch_len <= len {
                        if let Ok(s) = std::str::from_utf8(&bytes[i..i + ch_len]) {
                            current.push_str(s);
                        }
                    }
                    i += ch_len;
                }
            }
            _ => {
                // Consume a full UTF-8 character (1-4 bytes)
                let ch_len = utf8_char_len(bytes[i]);
                if i + ch_len <= len {
                    if let Ok(s) = std::str::from_utf8(&bytes[i..i + ch_len]) {
                        current.push_str(s);
                    }
                }
                i += ch_len;
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

/// Apply ANSI SGR parameters to formatting state.
fn apply_ansi_params(
    params: &[u8],
    bold: &mut bool,
    italic: &mut bool,
    underline: &mut bool,
    reverse: &mut bool,
    fg: &mut Option<u8>,
    bg: &mut Option<u8>,
) {
    if params.is_empty() || (params.len() == 1 && params[0] == 0) {
        // Reset all
        *bold = false;
        *italic = false;
        *underline = false;
        *reverse = false;
        *fg = None;
        *bg = None;
        return;
    }
    for &p in params {
        match p {
            0 => {
                *bold = false; *italic = false; *underline = false; *reverse = false;
                *fg = None; *bg = None;
            }
            1 => *bold = true,
            3 => *italic = true,
            4 => *underline = true,
            7 => *reverse = true,
            22 => *bold = false,
            23 => *italic = false,
            24 => *underline = false,
            27 => *reverse = false,
            // Standard foreground colors (30-37) → mIRC approximate
            30 => *fg = Some(1),  // black
            31 => *fg = Some(4),  // red
            32 => *fg = Some(3),  // green
            33 => *fg = Some(8),  // yellow
            34 => *fg = Some(2),  // blue
            35 => *fg = Some(6),  // magenta
            36 => *fg = Some(10), // cyan
            37 => *fg = Some(0),  // white
            39 => *fg = None,     // default fg
            // Standard background colors (40-47) → mIRC approximate
            40 => *bg = Some(1),  // black
            41 => *bg = Some(4),  // red
            42 => *bg = Some(3),  // green
            43 => *bg = Some(8),  // yellow
            44 => *bg = Some(2),  // blue
            45 => *bg = Some(6),  // magenta
            46 => *bg = Some(10), // cyan
            47 => *bg = Some(0),  // white
            49 => *bg = None,     // default bg
            // Bright foreground (90-97)
            90 => *fg = Some(14), // dark gray
            91 => *fg = Some(4),  // light red
            92 => *fg = Some(9),  // light green
            93 => *fg = Some(8),  // light yellow
            94 => *fg = Some(12), // light blue
            95 => *fg = Some(13), // light magenta
            96 => *fg = Some(11), // light cyan
            97 => *fg = Some(15), // bright white
            _ => {} // ignore unknown
        }
    }
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
            0x1b => {
                // Skip ANSI escape sequences
                if i + 1 < len && bytes[i + 1] == b'[' {
                    i += 2;
                    while i < len && !bytes[i].is_ascii_alphabetic() { i += 1; }
                    if i < len { i += 1; } // skip terminator
                } else {
                    i += 1;
                }
            }
            _ => {
                let ch_len = utf8_char_len(bytes[i]);
                if i + ch_len <= len {
                    if let Ok(s) = std::str::from_utf8(&bytes[i..i + ch_len]) {
                        result.push_str(s);
                    }
                }
                i += ch_len;
            }
        }
    }
    result
}

/// Map a color name to its mIRC color code.
pub fn color_name_to_code(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "white" => Some(0),
        "black" => Some(1),
        "blue" | "navy" => Some(2),
        "green" => Some(3),
        "red" => Some(4),
        "brown" | "maroon" => Some(5),
        "purple" | "magenta" => Some(6),
        "orange" => Some(7),
        "yellow" => Some(8),
        "lime" | "lightgreen" => Some(9),
        "cyan" | "teal" => Some(10),
        "aqua" | "lightcyan" => Some(11),
        "lightblue" | "royal" => Some(12),
        "pink" | "lightpurple" | "fuchsia" => Some(13),
        "grey" | "gray" => Some(14),
        "lightgrey" | "lightgray" | "silver" => Some(15),
        _ => None,
    }
}

/// List all named colors for /colors command.
pub fn color_names() -> Vec<(&'static str, u8)> {
    vec![
        ("white", 0), ("black", 1), ("blue", 2), ("green", 3),
        ("red", 4), ("brown", 5), ("purple", 6), ("orange", 7),
        ("yellow", 8), ("lime", 9), ("cyan", 10), ("aqua", 11),
        ("lightblue", 12), ("pink", 13), ("grey", 14), ("silver", 15),
    ]
}

/// Convert user-friendly format shortcuts to IRC control codes.
/// %B = bold, %I = italic, %U = underline, %R = reverse, %O = reset
/// %C<fg>[,<bg>] = color by number or name
///   %C4 = red, %C4,1 = red on black
///   %Cred = red, %Cred,black = red on black
/// %<combo>% = user-defined combo (static or cycle)
///
/// If no combos are needed, pass an empty map.
pub fn apply_input_shortcuts(
    text: &str,
    combos: &std::collections::HashMap<String, crate::config::combos::ComboDefinition>,
) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '%' && i + 1 < len {
            // Try combo expansion first (%name%) — combos take priority
            if !combos.is_empty() {
                if let Some((expanded, consumed)) = try_expand_combo(&chars, i, combos) {
                    result.push_str(&expanded);
                    i += consumed;
                    continue;
                }
            }
            match chars[i + 1] {
                'B' | 'b' => { result.push('\x02'); i += 2; }
                'I' | 'i' => { result.push('\x1d'); i += 2; }
                'U' | 'u' => { result.push('\x1f'); i += 2; }
                'R' | 'r' => { result.push('\x16'); i += 2; }
                'O' | 'o' => { result.push('\x0f'); i += 2; }
                'C' | 'c' => {
                    i += 2;
                    // Try named color first
                    let remaining: String = chars[i..].iter().collect();
                    if let Some((fg_code, consumed)) = try_parse_color_name(&remaining) {
                        result.push('\x03');
                        result.push_str(&fg_code.to_string());
                        i += consumed;
                        // Check for ,bg
                        if i < len && chars[i] == ',' {
                            i += 1;
                            let remaining: String = chars[i..].iter().collect();
                            if let Some((bg_code, consumed)) = try_parse_color_name(&remaining) {
                                result.push(',');
                                result.push_str(&bg_code.to_string());
                                i += consumed;
                            }
                        }
                    } else {
                        // Numeric color
                        result.push('\x03');
                        while i < len && chars[i].is_ascii_digit() { result.push(chars[i]); i += 1; }
                        if i < len && chars[i] == ',' {
                            result.push(',');
                            i += 1;
                            while i < len && chars[i].is_ascii_digit() { result.push(chars[i]); i += 1; }
                        }
                    }
                }
                '%' => { result.push('%'); i += 2; }
                _ => { result.push('%'); i += 1; }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Try to match %<name>% at position `start` in chars. Returns (expanded, chars_consumed).
fn try_expand_combo(
    chars: &[char],
    start: usize,
    combos: &std::collections::HashMap<String, crate::config::combos::ComboDefinition>,
) -> Option<(String, usize)> {
    use crate::config::combos::ComboDefinition;

    // start points at the opening '%', next char is the first of the name
    let name_start = start + 1;
    // Find the closing '%'
    let mut j = name_start;
    while j < chars.len() && chars[j] != '%' {
        // Combo names are alphanumeric + underscore
        if !chars[j].is_alphanumeric() && chars[j] != '_' {
            return None;
        }
        j += 1;
    }
    if j >= chars.len() || j == name_start {
        return None; // No closing % or empty name
    }

    let name: String = chars[name_start..j].iter().collect();
    let name_lower = name.to_lowercase();
    let combo = combos.get(&name_lower)?;
    let consumed = j + 1 - start; // includes both % delimiters

    match combo {
        ComboDefinition::Static(fmt) => {
            // Expand the static format string using basic shortcuts only (no recursion into combos)
            let expanded = apply_input_shortcuts_basic(fmt);
            Some((expanded, consumed))
        }
        ComboDefinition::Dynamic(dynamic) if dynamic.combo_type == "cycle" => {
            // Find the text to colorize: everything from after %name% to %O or end
            let text_start = start + consumed;
            let mut text_end = text_start;
            while text_end < chars.len() {
                // Check for %O (reset) which ends the cycle region
                if chars[text_end] == '%'
                    && text_end + 1 < chars.len()
                    && (chars[text_end + 1] == 'O' || chars[text_end + 1] == 'o')
                {
                    break;
                }
                text_end += 1;
            }

            let text_chars: Vec<char> = chars[text_start..text_end].to_vec();
            let color_codes: Vec<u8> = dynamic
                .colors
                .iter()
                .filter_map(|name| color_name_to_code(name).or_else(|| name.parse::<u8>().ok()))
                .collect();

            if color_codes.is_empty() {
                return Some((String::new(), consumed));
            }

            let mut expanded = String::new();
            let mut color_idx = 0;
            for ch in &text_chars {
                if !ch.is_whitespace() {
                    expanded.push('\x03');
                    expanded.push_str(&color_codes[color_idx % color_codes.len()].to_string());
                    color_idx += 1;
                }
                expanded.push(*ch);
            }
            expanded.push('\x0f'); // Reset after cycle

            // Total consumed: %name% + text + %O (if present)
            let total_consumed = if text_end < chars.len()
                && chars[text_end] == '%'
                && text_end + 1 < chars.len()
            {
                text_end + 2 - start // includes the %O
            } else {
                text_end - start // no %O, just the text
            };

            Some((expanded, total_consumed))
        }
        _ => None, // Unknown dynamic type
    }
}

/// Apply only basic format shortcuts (no combo expansion). Used by static combos
/// to prevent infinite recursion.
fn apply_input_shortcuts_basic(text: &str) -> String {
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
                    i += 2;
                    let remaining: String = chars[i..].iter().collect();
                    if let Some((fg_code, consumed)) = try_parse_color_name(&remaining) {
                        result.push('\x03');
                        result.push_str(&fg_code.to_string());
                        i += consumed;
                        if i < len && chars[i] == ',' {
                            i += 1;
                            let remaining: String = chars[i..].iter().collect();
                            if let Some((bg_code, consumed)) = try_parse_color_name(&remaining) {
                                result.push(',');
                                result.push_str(&bg_code.to_string());
                                i += consumed;
                            }
                        }
                    } else {
                        result.push('\x03');
                        while i < len && chars[i].is_ascii_digit() { result.push(chars[i]); i += 1; }
                        if i < len && chars[i] == ',' {
                            result.push(',');
                            i += 1;
                            while i < len && chars[i].is_ascii_digit() { result.push(chars[i]); i += 1; }
                        }
                    }
                }
                '%' => { result.push('%'); i += 2; }
                _ => { result.push('%'); i += 1; }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Try to parse a color name at the start of a string. Returns (code, chars_consumed).
fn try_parse_color_name(s: &str) -> Option<(u8, usize)> {
    // Try longest names first
    for &(name, code) in &[
        ("lightgreen", 9u8), ("lightcyan", 11), ("lightblue", 12),
        ("lightpurple", 13), ("lightgrey", 14), ("lightgray", 14),
        ("magenta", 6), ("maroon", 5), ("fuchsia", 13),
        ("orange", 7), ("yellow", 8), ("purple", 6),
        ("silver", 15), ("brown", 5), ("green", 3),
        ("white", 0), ("black", 1), ("blue", 2), ("navy", 2),
        ("aqua", 11), ("cyan", 10), ("teal", 10), ("lime", 9),
        ("pink", 13), ("grey", 14), ("gray", 14), ("red", 4),
        ("royal", 12),
    ] {
        if s.to_lowercase().starts_with(name) {
            // Make sure next char isn't alphanumeric (word boundary)
            let next = s.chars().nth(name.len());
            if next.is_none() || !next.unwrap().is_alphanumeric() {
                return Some((code, name.len()));
            }
        }
    }
    None
}

/// Determine the length of a UTF-8 character from its first byte.
fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1, // invalid leading byte, advance 1
    }
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

    fn no_combos() -> std::collections::HashMap<String, crate::config::combos::ComboDefinition> {
        std::collections::HashMap::new()
    }

    #[test]
    fn input_shortcuts() {
        let c = no_combos();
        assert_eq!(apply_input_shortcuts("%Bbold%B", &c), "\x02bold\x02");
        assert_eq!(apply_input_shortcuts("%C4red%O", &c), "\x034red\x0f");
        assert_eq!(apply_input_shortcuts("%C4,1red on black", &c), "\x034,1red on black");
        assert_eq!(apply_input_shortcuts("100%%", &c), "100%");
    }

    #[test]
    fn named_color_shortcuts() {
        let c = no_combos();
        assert_eq!(apply_input_shortcuts("%Cred hello%O", &c), "\x034 hello\x0f");
        assert_eq!(apply_input_shortcuts("%Cblue,white text", &c), "\x032,0 text");
        assert_eq!(apply_input_shortcuts("%Cgreen ok", &c), "\x033 ok");
    }

    #[test]
    fn color_name_lookup() {
        assert_eq!(color_name_to_code("red"), Some(4));
        assert_eq!(color_name_to_code("RED"), Some(4));
        assert_eq!(color_name_to_code("blue"), Some(2));
        assert_eq!(color_name_to_code("notacolor"), None);
    }

    #[test]
    fn plain_text_unchanged() {
        let spans = parse_irc_format("hello world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert!(!spans[0].bold);
        assert_eq!(spans[0].fg, None);
    }

    #[test]
    fn static_combo_expansion() {
        use crate::config::combos::ComboDefinition;
        let mut combos = std::collections::HashMap::new();
        combos.insert("alert".to_string(), ComboDefinition::Static("%B%Cred,white".to_string()));
        let result = apply_input_shortcuts("%alert%WARNING%O", &combos);
        // %alert% expands to bold + red,white, then WARNING, then reset
        assert_eq!(result, "\x02\x034,0WARNING\x0f");
    }

    #[test]
    fn cycle_combo_expansion() {
        use crate::config::combos::{ComboDefinition, DynamicCombo};
        let mut combos = std::collections::HashMap::new();
        combos.insert("test".to_string(), ComboDefinition::Dynamic(DynamicCombo {
            combo_type: "cycle".to_string(),
            colors: vec!["red".into(), "blue".into()],
        }));
        let result = apply_input_shortcuts("%test%ab%O", &combos);
        // 'a' gets red (4), 'b' gets blue (2), then reset
        assert_eq!(result, "\x034a\x032b\x0f");
    }

    #[test]
    fn cycle_combo_preserves_spaces() {
        use crate::config::combos::{ComboDefinition, DynamicCombo};
        let mut combos = std::collections::HashMap::new();
        combos.insert("rb".to_string(), ComboDefinition::Dynamic(DynamicCombo {
            combo_type: "cycle".to_string(),
            colors: vec!["red".into(), "blue".into()],
        }));
        let result = apply_input_shortcuts("%rb%a b%O", &combos);
        // 'a' gets red, space stays plain, 'b' gets blue
        assert_eq!(result, "\x034a \x032b\x0f");
    }

    #[test]
    fn combo_case_insensitive() {
        use crate::config::combos::ComboDefinition;
        let mut combos = std::collections::HashMap::new();
        combos.insert("alert".to_string(), ComboDefinition::Static("%B".to_string()));
        let result = apply_input_shortcuts("%Alert%hi%O", &combos);
        assert_eq!(result, "\x02hi\x0f");
    }

    #[test]
    fn unknown_combo_falls_through_to_shortcuts() {
        let c = no_combos();
        // %U is underline, so %unknown% starts with underline shortcut
        let result = apply_input_shortcuts("%unknown%text", &c);
        assert!(result.starts_with("\x1f")); // underline from %U
    }

    #[test]
    fn unknown_name_no_shortcut_conflict() {
        let c = no_combos();
        // %xyz% — 'x' isn't a shortcut, so % is literal
        assert_eq!(apply_input_shortcuts("%xyz%text", &c), "%xyz%text");
    }
}
