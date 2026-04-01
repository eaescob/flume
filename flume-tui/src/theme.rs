use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::SystemTime;

use ratatui::style::Color;

use flume_core::config::theme::{self, ThemeConfig};

/// Resolved theme with pre-parsed ratatui Color values.
/// Built from a ThemeConfig (TOML deserialization layer).
pub struct Theme {
    /// The config this was built from (kept for serialization/debugging).
    pub name: String,

    // ── Bar colors ──
    pub title_bar_bg: Color,
    pub title_bar_fg: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,

    // ── Input line ──
    pub input_bg: Color,
    pub input_fg: Color,

    // ── Buffer list ──
    pub buffer_list_bg: Color,
    pub buffer_list_fg: Color,

    // ── Nick list ──
    pub nick_list_bg: Color,
    pub nick_list_fg: Color,
    pub nick_list_op: Color,
    pub nick_list_voice: Color,

    // ── Chat buffer ──
    pub chat_timestamp: Color,
    pub chat_nick: Color,
    pub chat_message: Color,
    pub chat_own_nick: Color,
    pub chat_action: Color,
    pub chat_notice: Color,
    pub chat_server: Color,
    pub chat_system: Color,
    pub chat_highlight: Color,
    pub chat_url: Color,

    // ── Semantic UI colors ──
    pub unread: Color,
    pub inactive: Color,
    pub active: Color,
    pub scroll_indicator: Color,
    pub search_match_bg: Color,
    pub search_match_fg: Color,
    pub state_connected: Color,
    pub state_connecting: Color,
    pub state_disconnected: Color,

    // ── Nick palette ──
    nick_palette: Vec<Color>,

    // ── Hot-reload tracking ──
    theme_file: Option<PathBuf>,
    last_mtime: Option<SystemTime>,
}

impl Theme {
    /// Build a Theme from a ThemeConfig by resolving all color strings.
    pub fn from_config(config: &ThemeConfig) -> Self {
        let e = &config.elements;
        Theme {
            name: config.meta.name.clone(),
            title_bar_bg: resolve(&e.title_bar_bg),
            title_bar_fg: resolve(&e.title_bar_fg),
            status_bar_bg: resolve(&e.status_bar_bg),
            status_bar_fg: resolve(&e.status_bar_fg),
            input_bg: resolve(&e.input_bg),
            input_fg: resolve(&e.input_fg),
            // Buffer list defaults to title bar colors if not specified
            buffer_list_bg: resolve(&e.title_bar_bg),
            buffer_list_fg: resolve(&e.status_bar_fg),
            nick_list_bg: resolve(&e.nick_list_bg),
            nick_list_fg: resolve(&e.nick_list_fg),
            nick_list_op: resolve(&e.nick_list_op),
            nick_list_voice: resolve(&e.nick_list_voice),
            chat_timestamp: resolve(&e.chat_timestamp),
            chat_nick: resolve(&e.chat_nick),
            chat_message: resolve(&e.chat_message),
            chat_own_nick: resolve(&e.chat_own_nick),
            chat_action: resolve(&e.chat_action),
            chat_notice: resolve(&e.chat_notice),
            chat_server: resolve(&e.chat_server),
            chat_system: resolve(&e.chat_system),
            chat_highlight: resolve(&e.chat_highlight),
            chat_url: resolve(&e.chat_url),
            unread: resolve(&e.unread),
            inactive: resolve(&e.inactive),
            active: resolve(&e.active),
            scroll_indicator: resolve(&e.scroll_indicator),
            search_match_bg: resolve(&e.search_match_bg),
            search_match_fg: resolve(&e.search_match_fg),
            state_connected: resolve(&e.state_connected),
            state_connecting: resolve(&e.state_connecting),
            state_disconnected: resolve(&e.state_disconnected),
            nick_palette: config
                .nick_colors
                .palette
                .iter()
                .map(|s| resolve(s))
                .collect(),
            theme_file: None,
            last_mtime: None,
        }
    }

    /// Load theme by name from the themes directory.
    /// Returns default theme if name is "default" or the file doesn't exist.
    pub fn load(name: &str) -> Self {
        if name == "default" {
            return Self::default_theme();
        }

        let path = flume_core::config::themes_dir().join(format!("{}.toml", name));
        match theme::load_theme_config(&path) {
            Ok(config) => {
                let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                let mut theme = Self::from_config(&config);
                theme.theme_file = Some(path);
                theme.last_mtime = mtime;
                theme
            }
            Err(e) => {
                tracing::warn!("Failed to load theme '{}': {}, using default", name, e);
                Self::default_theme()
            }
        }
    }

    /// The built-in default theme matching the original hardcoded colors.
    pub fn default_theme() -> Self {
        Self::from_config(&ThemeConfig::default())
    }

    /// Check if the theme file has been modified and reload if so.
    /// Returns true if the theme was reloaded.
    pub fn check_reload(&mut self) -> bool {
        let path = match &self.theme_file {
            Some(p) => p.clone(),
            None => return false,
        };

        let current_mtime = match std::fs::metadata(&path).ok().and_then(|m| m.modified().ok()) {
            Some(t) => t,
            None => return false,
        };

        if self.last_mtime == Some(current_mtime) {
            return false;
        }

        match theme::load_theme_config(&path) {
            Ok(config) => {
                let reloaded = Self::from_config(&config);
                self.name = reloaded.name;
                self.title_bar_bg = reloaded.title_bar_bg;
                self.title_bar_fg = reloaded.title_bar_fg;
                self.status_bar_bg = reloaded.status_bar_bg;
                self.status_bar_fg = reloaded.status_bar_fg;
                self.input_bg = reloaded.input_bg;
                self.input_fg = reloaded.input_fg;
                self.buffer_list_bg = reloaded.buffer_list_bg;
                self.buffer_list_fg = reloaded.buffer_list_fg;
                self.nick_list_bg = reloaded.nick_list_bg;
                self.nick_list_fg = reloaded.nick_list_fg;
                self.nick_list_op = reloaded.nick_list_op;
                self.nick_list_voice = reloaded.nick_list_voice;
                self.chat_timestamp = reloaded.chat_timestamp;
                self.chat_nick = reloaded.chat_nick;
                self.chat_message = reloaded.chat_message;
                self.chat_own_nick = reloaded.chat_own_nick;
                self.chat_action = reloaded.chat_action;
                self.chat_notice = reloaded.chat_notice;
                self.chat_server = reloaded.chat_server;
                self.chat_system = reloaded.chat_system;
                self.chat_highlight = reloaded.chat_highlight;
                self.chat_url = reloaded.chat_url;
                self.unread = reloaded.unread;
                self.inactive = reloaded.inactive;
                self.active = reloaded.active;
                self.scroll_indicator = reloaded.scroll_indicator;
                self.search_match_bg = reloaded.search_match_bg;
                self.search_match_fg = reloaded.search_match_fg;
                self.state_connected = reloaded.state_connected;
                self.state_connecting = reloaded.state_connecting;
                self.state_disconnected = reloaded.state_disconnected;
                self.nick_palette = reloaded.nick_palette;
                self.last_mtime = Some(current_mtime);
                tracing::info!("Theme '{}' reloaded", self.name);
                true
            }
            Err(e) => {
                tracing::warn!("Failed to reload theme: {}", e);
                false
            }
        }
    }

    /// Switch to a different theme by name.
    pub fn switch_to(&mut self, name: &str) {
        let new_theme = Self::load(name);
        *self = new_theme;
    }

    /// Force a reload of the current theme file. Returns true if reloaded.
    pub fn force_reload(&mut self) -> bool {
        self.last_mtime = None;
        self.check_reload()
    }

    /// Whether this theme is loaded from a file (vs default).
    pub fn has_file(&self) -> bool {
        self.theme_file.is_some()
    }

    /// Get the theme file path (if loaded from a file).
    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.theme_file.as_deref()
    }

    /// Get a deterministic color for a nick by hashing it into the palette.
    pub fn nick_color(&self, nick: &str) -> Color {
        if self.nick_palette.is_empty() {
            return self.chat_nick;
        }
        let mut hasher = DefaultHasher::new();
        nick.hash(&mut hasher);
        let hash = hasher.finish();
        self.nick_palette[(hash as usize) % self.nick_palette.len()]
    }

    /// List available theme names from the themes directory.
    pub fn list_available() -> Vec<String> {
        let mut names = vec!["default".to_string()];
        let dir = flume_core::config::themes_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "toml") {
                    if let Some(stem) = path.file_stem() {
                        let name = stem.to_string_lossy().to_string();
                        if name != "default" {
                            names.push(name);
                        }
                    }
                }
            }
        }
        names.sort();
        names
    }
}

/// Resolve a color string to a ratatui Color.
fn resolve(s: &str) -> Color {
    // Try hex first
    if let Some((r, g, b)) = theme::parse_color_rgb(s) {
        return Color::Rgb(r, g, b);
    }

    // Try named color
    match theme::named_color(s) {
        Some("reset") | Some("default") => Color::Reset,
        Some("black") => Color::Black,
        Some("red") => Color::Red,
        Some("green") => Color::Green,
        Some("yellow") => Color::Yellow,
        Some("blue") => Color::Blue,
        Some("magenta") => Color::Magenta,
        Some("cyan") => Color::Cyan,
        Some("gray") => Color::Gray,
        Some("darkgray") => Color::DarkGray,
        Some("lightred") => Color::LightRed,
        Some("lightgreen") => Color::LightGreen,
        Some("lightyellow") => Color::LightYellow,
        Some("lightblue") => Color::LightBlue,
        Some("lightmagenta") => Color::LightMagenta,
        Some("lightcyan") => Color::LightCyan,
        Some("white") => Color::White,
        _ => {
            tracing::warn!("Unknown color '{}', using Reset", s);
            Color::Reset
        }
    }
}
