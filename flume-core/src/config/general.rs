use serde::{Deserialize, Serialize};

use super::dcc::DccConfig;
use super::keybindings::KeybindingsConfig;
use super::llm::LlmConfig;

/// Top-level Flume configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FlumeConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub ctcp: CtcpConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub dcc: DccConfig,
}

impl Default for FlumeConfig {
    fn default() -> Self {
        FlumeConfig {
            general: GeneralConfig::default(),
            ui: UiConfig::default(),
            logging: LoggingConfig::default(),
            notifications: NotificationConfig::default(),
            ctcp: CtcpConfig::default(),
            llm: LlmConfig::default(),
            dcc: DccConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "default_nick")]
    pub default_nick: String,
    #[serde(default)]
    pub alt_nicks: Vec<String>,
    #[serde(default = "default_realname")]
    pub realname: String,
    #[serde(default = "default_username")]
    pub username: String,
    #[serde(default = "default_quit_message")]
    pub quit_message: String,
    #[serde(default = "default_timestamp_format")]
    pub timestamp_format: String,
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: usize,
    #[serde(default = "default_url_open_command")]
    pub url_open_command: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            default_nick: default_nick(),
            alt_nicks: Vec::new(),
            realname: default_realname(),
            username: default_username(),
            quit_message: default_quit_message(),
            timestamp_format: default_timestamp_format(),
            scrollback_lines: default_scrollback_lines(),
            url_open_command: default_url_open_command(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_true")]
    pub show_server_tree: bool,
    #[serde(default = "default_true")]
    pub show_nick_list: bool,
    #[serde(default = "default_server_tree_width")]
    pub server_tree_width: u16,
    #[serde(default = "default_nick_list_width")]
    pub nick_list_width: u16,
    #[serde(default = "default_input_history_size")]
    pub input_history_size: usize,
    #[serde(default = "default_tick_rate")]
    pub tick_rate_fps: u32,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            theme: default_theme(),
            layout: default_layout(),
            show_server_tree: true,
            show_nick_list: true,
            server_tree_width: default_server_tree_width(),
            nick_list_width: default_nick_list_width(),
            input_history_size: default_input_history_size(),
            tick_rate_fps: default_tick_rate(),
            keybindings: KeybindingsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_log_rotate")]
    pub rotate: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            enabled: true,
            format: default_log_format(),
            rotate: default_log_rotate(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub highlight_bell: bool,
    #[serde(default)]
    pub highlight_words: Vec<String>,
    #[serde(default = "default_true")]
    pub notify_private: bool,
    #[serde(default = "default_true")]
    pub notify_highlight: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        NotificationConfig {
            highlight_bell: true,
            highlight_words: Vec::new(),
            notify_private: true,
            notify_highlight: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CtcpConfig {
    #[serde(default = "default_version_reply")]
    pub version_reply: String,
    #[serde(default = "default_true")]
    pub respond_to_version: bool,
    #[serde(default = "default_true")]
    pub respond_to_ping: bool,
    #[serde(default = "default_true")]
    pub respond_to_time: bool,
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
}

impl Default for CtcpConfig {
    fn default() -> Self {
        CtcpConfig {
            version_reply: default_version_reply(),
            respond_to_version: true,
            respond_to_ping: true,
            respond_to_time: true,
            rate_limit: default_rate_limit(),
        }
    }
}

// Default value functions for serde
fn default_nick() -> String { "flume_user".to_string() }
fn default_realname() -> String { "Flume User".to_string() }
fn default_username() -> String { "flume".to_string() }
fn default_quit_message() -> String { "Flume IRC — https://github.com/emilio/flume".to_string() }
fn default_timestamp_format() -> String { "%H:%M:%S".to_string() }
fn default_scrollback_lines() -> usize { 10000 }
fn default_url_open_command() -> String {
    if cfg!(target_os = "macos") { "open".to_string() } else { "xdg-open".to_string() }
}
fn default_theme() -> String { "default".to_string() }
fn default_layout() -> String { "default".to_string() }
fn default_true() -> bool { true }
fn default_server_tree_width() -> u16 { 20 }
fn default_nick_list_width() -> u16 { 18 }
fn default_input_history_size() -> usize { 500 }
fn default_tick_rate() -> u32 { 30 }
fn default_log_format() -> String { "plain".to_string() }
fn default_log_rotate() -> String { "daily".to_string() }
fn default_version_reply() -> String { format!("Flume {}", env!("CARGO_PKG_VERSION")) }
fn default_rate_limit() -> u32 { 3 }
