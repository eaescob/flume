use serde::{Deserialize, Serialize};

/// Theme configuration loaded from a TOML file.
///
/// Theme files live in `~/.config/flume/themes/<name>.toml`.
/// The special name "default" uses the built-in default theme.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ThemeConfig {
    #[serde(default)]
    pub meta: ThemeMeta,
    #[serde(default)]
    pub colors: ThemeColors,
    #[serde(default)]
    pub nick_colors: NickColorConfig,
    #[serde(default)]
    pub elements: ElementColors,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeMeta {
    #[serde(default = "default_theme_name")]
    pub name: String,
    #[serde(default)]
    pub author: String,
}

impl Default for ThemeMeta {
    fn default() -> Self {
        ThemeMeta {
            name: default_theme_name(),
            author: String::new(),
        }
    }
}

/// Base semantic colors used throughout the UI.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeColors {
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_foreground")]
    pub foreground: String,
    #[serde(default = "default_highlight")]
    pub highlight: String,
    #[serde(default = "default_error")]
    pub error: String,
    #[serde(default = "default_warning")]
    pub warning: String,
    #[serde(default = "default_success")]
    pub success: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        ThemeColors {
            background: default_background(),
            foreground: default_foreground(),
            highlight: default_highlight(),
            error: default_error(),
            warning: default_warning(),
            success: default_success(),
        }
    }
}

/// Nick color palette — nicks are colored by hashing into this palette.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NickColorConfig {
    #[serde(default = "default_nick_palette")]
    pub palette: Vec<String>,
}

impl Default for NickColorConfig {
    fn default() -> Self {
        NickColorConfig {
            palette: default_nick_palette(),
        }
    }
}

/// Per-element UI colors.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElementColors {
    #[serde(default = "default_title_bar_bg")]
    pub title_bar_bg: String,
    #[serde(default = "default_title_bar_fg")]
    pub title_bar_fg: String,
    #[serde(default = "default_status_bar_bg")]
    pub status_bar_bg: String,
    #[serde(default = "default_status_bar_fg")]
    pub status_bar_fg: String,
    #[serde(default = "default_input_bg")]
    pub input_bg: String,
    #[serde(default = "default_input_fg")]
    pub input_fg: String,
    #[serde(default = "default_nick_list_bg")]
    pub nick_list_bg: String,
    #[serde(default = "default_nick_list_fg")]
    pub nick_list_fg: String,
    #[serde(default = "default_nick_list_op")]
    pub nick_list_op: String,
    #[serde(default = "default_nick_list_voice")]
    pub nick_list_voice: String,
    #[serde(default = "default_chat_timestamp")]
    pub chat_timestamp: String,
    #[serde(default = "default_chat_nick")]
    pub chat_nick: String,
    #[serde(default = "default_chat_message")]
    pub chat_message: String,
    #[serde(default = "default_chat_own_nick")]
    pub chat_own_nick: String,
    #[serde(default = "default_chat_action")]
    pub chat_action: String,
    #[serde(default = "default_chat_notice")]
    pub chat_notice: String,
    #[serde(default = "default_chat_server")]
    pub chat_server: String,
    #[serde(default = "default_chat_system")]
    pub chat_system: String,
    #[serde(default = "default_chat_highlight")]
    pub chat_highlight: String,
    #[serde(default = "default_chat_url")]
    pub chat_url: String,
    #[serde(default = "default_unread")]
    pub unread: String,
    #[serde(default = "default_inactive")]
    pub inactive: String,
    #[serde(default = "default_active")]
    pub active: String,
    #[serde(default = "default_scroll_indicator")]
    pub scroll_indicator: String,
    #[serde(default = "default_search_match")]
    pub search_match_bg: String,
    #[serde(default = "default_search_match_fg")]
    pub search_match_fg: String,
    #[serde(default = "default_state_connected")]
    pub state_connected: String,
    #[serde(default = "default_state_connecting")]
    pub state_connecting: String,
    #[serde(default = "default_state_disconnected")]
    pub state_disconnected: String,
}

impl Default for ElementColors {
    fn default() -> Self {
        ElementColors {
            title_bar_bg: default_title_bar_bg(),
            title_bar_fg: default_title_bar_fg(),
            status_bar_bg: default_status_bar_bg(),
            status_bar_fg: default_status_bar_fg(),
            input_bg: default_input_bg(),
            input_fg: default_input_fg(),
            nick_list_bg: default_nick_list_bg(),
            nick_list_fg: default_nick_list_fg(),
            nick_list_op: default_nick_list_op(),
            nick_list_voice: default_nick_list_voice(),
            chat_timestamp: default_chat_timestamp(),
            chat_nick: default_chat_nick(),
            chat_message: default_chat_message(),
            chat_own_nick: default_chat_own_nick(),
            chat_action: default_chat_action(),
            chat_notice: default_chat_notice(),
            chat_server: default_chat_server(),
            chat_system: default_chat_system(),
            chat_highlight: default_chat_highlight(),
            chat_url: default_chat_url(),
            unread: default_unread(),
            inactive: default_inactive(),
            active: default_active(),
            scroll_indicator: default_scroll_indicator(),
            search_match_bg: default_search_match(),
            search_match_fg: default_search_match_fg(),
            state_connected: default_state_connected(),
            state_connecting: default_state_connecting(),
            state_disconnected: default_state_disconnected(),
        }
    }
}

/// Parse a color string into an (r, g, b) tuple.
///
/// Supports:
/// - Hex colors: "#rrggbb" or "#rgb"
/// - Named colors: "red", "green", "blue", "cyan", "magenta", "yellow",
///   "white", "black", "darkgray", "gray", "lightred", "lightgreen",
///   "lightblue", "lightcyan", "lightmagenta", "lightyellow"
/// - "reset" for terminal default
///
/// Returns None for unrecognized colors.
pub fn parse_color_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some((r, g, b))
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some((r, g, b))
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Named color constant — maps a name to an ANSI color index.
/// Returns None if the name isn't recognized as a named color or "reset".
pub fn named_color(s: &str) -> Option<&'static str> {
    match s.trim().to_lowercase().as_str() {
        "reset" | "default" => Some("reset"),
        "black" => Some("black"),
        "red" => Some("red"),
        "green" => Some("green"),
        "yellow" => Some("yellow"),
        "blue" => Some("blue"),
        "magenta" => Some("magenta"),
        "cyan" => Some("cyan"),
        "gray" | "grey" => Some("gray"),
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Some("darkgray"),
        "lightred" | "light_red" => Some("lightred"),
        "lightgreen" | "light_green" => Some("lightgreen"),
        "lightyellow" | "light_yellow" => Some("lightyellow"),
        "lightblue" | "light_blue" => Some("lightblue"),
        "lightmagenta" | "light_magenta" => Some("lightmagenta"),
        "lightcyan" | "light_cyan" => Some("lightcyan"),
        "white" => Some("white"),
        _ => None,
    }
}

/// Load a theme config from a TOML file path.
pub fn load_theme_config(path: &std::path::Path) -> Result<ThemeConfig, super::ConfigError> {
    let contents = std::fs::read_to_string(path)?;
    let config: ThemeConfig = toml::from_str(&contents)?;
    Ok(config)
}

// ── Default values matching current hardcoded colors ──

fn default_theme_name() -> String { "Default".to_string() }

// Base colors
fn default_background() -> String { "reset".to_string() }
fn default_foreground() -> String { "white".to_string() }
fn default_highlight() -> String { "yellow".to_string() }
fn default_error() -> String { "red".to_string() }
fn default_warning() -> String { "yellow".to_string() }
fn default_success() -> String { "green".to_string() }

// Nick palette — 8 distinct colors for nick hashing
fn default_nick_palette() -> Vec<String> {
    vec![
        "cyan".to_string(),
        "green".to_string(),
        "magenta".to_string(),
        "yellow".to_string(),
        "lightred".to_string(),
        "lightblue".to_string(),
        "lightcyan".to_string(),
        "lightmagenta".to_string(),
    ]
}

// Element defaults — match current hardcoded Color::* values
fn default_title_bar_bg() -> String { "darkgray".to_string() }
fn default_title_bar_fg() -> String { "cyan".to_string() }
fn default_status_bar_bg() -> String { "darkgray".to_string() }
fn default_status_bar_fg() -> String { "white".to_string() }
fn default_input_bg() -> String { "reset".to_string() }
fn default_input_fg() -> String { "cyan".to_string() }
fn default_nick_list_bg() -> String { "reset".to_string() }
fn default_nick_list_fg() -> String { "darkgray".to_string() }
fn default_nick_list_op() -> String { "green".to_string() }
fn default_nick_list_voice() -> String { "cyan".to_string() }
fn default_chat_timestamp() -> String { "darkgray".to_string() }
fn default_chat_nick() -> String { "cyan".to_string() }
fn default_chat_message() -> String { "reset".to_string() }
fn default_chat_own_nick() -> String { "green".to_string() }
fn default_chat_action() -> String { "magenta".to_string() }
fn default_chat_notice() -> String { "blue".to_string() }
fn default_chat_server() -> String { "blue".to_string() }
fn default_chat_system() -> String { "yellow".to_string() }
fn default_chat_highlight() -> String { "yellow".to_string() }
fn default_chat_url() -> String { "lightblue".to_string() }
fn default_unread() -> String { "yellow".to_string() }
fn default_inactive() -> String { "darkgray".to_string() }
fn default_active() -> String { "cyan".to_string() }
fn default_scroll_indicator() -> String { "yellow".to_string() }
fn default_search_match() -> String { "darkgray".to_string() }
fn default_search_match_fg() -> String { "white".to_string() }
fn default_state_connected() -> String { "green".to_string() }
fn default_state_connecting() -> String { "yellow".to_string() }
fn default_state_disconnected() -> String { "red".to_string() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color() {
        assert_eq!(parse_color_rgb("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_color_rgb("#00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_color_rgb("#0000ff"), Some((0, 0, 255)));
        assert_eq!(parse_color_rgb("#002b36"), Some((0, 43, 54)));
    }

    #[test]
    fn parse_short_hex_color() {
        assert_eq!(parse_color_rgb("#f00"), Some((255, 0, 0)));
        assert_eq!(parse_color_rgb("#0f0"), Some((0, 255, 0)));
    }

    #[test]
    fn parse_invalid_color() {
        assert_eq!(parse_color_rgb("#xyz"), None);
        assert_eq!(parse_color_rgb("#12345"), None);
        assert_eq!(parse_color_rgb(""), None);
    }

    #[test]
    fn named_colors_recognized() {
        assert_eq!(named_color("reset"), Some("reset"));
        assert_eq!(named_color("Red"), Some("red"));
        assert_eq!(named_color("DarkGray"), Some("darkgray"));
        assert_eq!(named_color("light_cyan"), Some("lightcyan"));
        assert_eq!(named_color("bogus"), None);
    }

    #[test]
    fn default_theme_round_trip() {
        let config = ThemeConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: ThemeConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.elements.title_bar_bg, "darkgray");
        assert_eq!(parsed.elements.chat_action, "magenta");
        assert_eq!(parsed.nick_colors.palette.len(), 8);
    }

    #[test]
    fn deserialize_partial_theme() {
        let toml_str = r##"
[meta]
name = "My Theme"

[elements]
title_bar_bg = "#073642"
chat_url = "#268bd2"
"##;
        let config: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.meta.name, "My Theme");
        assert_eq!(config.elements.title_bar_bg, "#073642");
        assert_eq!(config.elements.chat_url, "#268bd2");
        // Unspecified fields get defaults
        assert_eq!(config.elements.chat_action, "magenta");
    }
}
