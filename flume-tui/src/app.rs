use std::collections::{HashMap, VecDeque};

use tokio::sync::mpsc;

use flume_core::config::formats::FormatsConfig;
use flume_core::config::general::NotificationConfig;
use flume_core::config::keybindings::KeybindingMode;
use flume_core::config::IrcConfig;
use flume_core::event::{ConnectionState, IrcEvent, UserCommand};
use flume_core::format::{format_string, format_regex_captures};
use flume_core::fmt_vars;
use flume_core::irc::command::Command;

/// A compiled snotice routing rule.
pub struct CompiledSnoticeRule {
    pub regex: regex::Regex,
    pub format: Option<String>,
    pub buffer: Option<String>,
    pub suppress: bool,
}

/// Compile snotice rule configs into regex-ready rules.
/// Check if a name is an IRC channel (starts with #, &, +, or !).
pub fn is_channel(name: &str) -> bool {
    matches!(name.as_bytes().first(), Some(b'#' | b'&' | b'+' | b'!'))
}

pub fn compile_snotice_rules(configs: &[flume_core::config::formats::SnoticeRuleConfig]) -> Vec<CompiledSnoticeRule> {
    configs
        .iter()
        .filter_map(|rule| {
            regex::Regex::new(&rule.pattern).ok().map(|re| CompiledSnoticeRule {
                regex: re,
                format: rule.format.clone(),
                buffer: rule.buffer.clone(),
                suppress: rule.suppress,
            })
        })
        .collect()
}

use flume_core::dcc::{DccTransfer, DccTransferState};

use crate::split::{SplitDirection, SplitState};

/// A notification event produced by incoming IRC messages.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum NotificationEvent {
    Highlight {
        server: String,
        buffer: String,
        nick: String,
        text: String,
    },
    PrivateMessage {
        server: String,
        nick: String,
        text: String,
    },
}

/// A displayable message in a buffer.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: MessageSource,
    pub text: String,
    pub highlight: bool,
}

/// Who sent a message.
#[derive(Debug, Clone)]
pub enum MessageSource {
    Server,
    User(String),
    Action(String),
    System,
    Own(String),
}

/// What kind of input the user is currently providing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Passphrase(String),
}

/// Vi sub-mode (only meaningful when keybinding mode is Vi).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViMode {
    Normal,
    Insert,
}

/// What kind of content was generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationKind {
    Script,
    Theme,
    Layout,
}

/// A pending LLM generation result awaiting user review.
#[derive(Debug, Clone)]
pub struct PendingGeneration {
    pub kind: GenerationKind,
    pub language: Option<String>,
    pub content: String,
    pub name: String,
    pub description: String,
}

/// A request to generate content via LLM (set by /generate, processed in main loop).
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub kind: GenerationKind,
    pub language: Option<String>,
    pub description: String,
    pub name: Option<String>,
}

/// A single message buffer (channel, PM, or server notices).
/// A nick in a channel with its prefix (e.g., "@" for op, "+" for voice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelNick {
    pub prefix: String, // e.g., "@", "+", "@+", ""
    pub nick: String,
}

impl ChannelNick {
    /// Sort key: ops first, then voiced, then regular.
    pub fn sort_key(&self) -> (u8, String) {
        let priority = if self.prefix.contains('@') {
            0
        } else if self.prefix.contains('+') {
            1
        } else {
            2
        };
        (priority, self.nick.to_lowercase())
    }
}

pub struct Buffer {
    pub messages: VecDeque<DisplayMessage>,
    pub scroll_offset: usize,
    pub unread_count: u32,
    pub highlight_count: u32,
    /// Nick list for channel buffers. Empty for non-channel buffers.
    pub nicks: Vec<ChannelNick>,
    /// Active search pattern (None = no search active).
    pub search: Option<String>,
    /// Channel topic (set by RPL_TOPIC 332 and TOPIC commands).
    pub topic: Option<String>,
    /// Channel modes (set by RPL_CHANNELMODEIS 324 and MODE commands).
    pub channel_modes: Option<String>,
}

impl Buffer {
    pub fn new() -> Self {
        Buffer {
            messages: VecDeque::new(),
            scroll_offset: 0,
            unread_count: 0,
            highlight_count: 0,
            nicks: Vec::new(),
            search: None,
            topic: None,
            channel_modes: None,
        }
    }

    /// Add a nick to the list (if not already present).
    pub fn add_nick(&mut self, prefix: &str, nick: &str) {
        if !self.nicks.iter().any(|n| n.nick == nick) {
            self.nicks.push(ChannelNick {
                prefix: prefix.to_string(),
                nick: nick.to_string(),
            });
            self.sort_nicks();
        }
    }

    /// Remove a nick from the list.
    pub fn remove_nick(&mut self, nick: &str) {
        self.nicks.retain(|n| n.nick != nick);
    }

    /// Rename a nick in the list.
    pub fn rename_nick(&mut self, old: &str, new: &str) {
        for n in &mut self.nicks {
            if n.nick == old {
                n.nick = new.to_string();
            }
        }
        self.sort_nicks();
    }

    fn sort_nicks(&mut self) {
        self.nicks.sort_by_key(|n| n.sort_key());
    }
}

/// Per-server state: nick, connection, and all buffers for that server.
pub struct ServerState {
    pub name: String,
    /// Buffers keyed by target: "" = server notices, "#channel", "nick" for PMs.
    pub buffers: HashMap<String, Buffer>,
    /// Which buffer is currently displayed for this server.
    pub active_buffer: String,
    /// Ordered list of buffer names for cycling.
    pub buffer_order: Vec<String>,
    pub nick: String,
    pub user_modes: String, // e.g., "+iwx"
    pub connection_state: ConnectionState,
    pub command_tx: Option<mpsc::Sender<UserCommand>>,
    /// Server supports echo-message (our messages are echoed back).
    pub has_echo_message: bool,
    /// Recently sent messages (text, timestamp) for echo deduplication.
    /// Only the last few are kept to distinguish echoes from bouncer playback.
    pub recent_own_messages: VecDeque<(String, chrono::DateTime<chrono::Utc>)>,
    /// Last raw server notice text (for /snotice last), per server.
    pub last_raw_snotice: Option<String>,
}

impl ServerState {
    pub fn new(name: &str, nick: &str) -> Self {
        let mut buffers = HashMap::new();
        buffers.insert(String::new(), Buffer::new()); // server buffer
        ServerState {
            name: name.to_string(),
            buffers,
            active_buffer: String::new(),
            buffer_order: vec![String::new()],
            nick: nick.to_string(),
            user_modes: String::new(),
            connection_state: ConnectionState::Disconnected,
            command_tx: None,
            has_echo_message: false,
            recent_own_messages: VecDeque::new(),
            last_raw_snotice: None,
        }
    }

    pub fn active_buf(&self) -> &Buffer {
        self.buffers.get(&self.active_buffer).unwrap()
    }

    pub fn active_buf_mut(&mut self) -> &mut Buffer {
        self.buffers.get_mut(&self.active_buffer).unwrap()
    }

    /// Ensure a buffer exists, creating it if needed.
    /// Get a buffer by name (case-insensitive for channels).
    pub fn get_buffer(&self, name: &str) -> Option<&Buffer> {
        let key = Self::normalize_buffer_name(name);
        self.buffers.get(&key)
    }

    /// Get a mutable buffer by name (case-insensitive for channels).
    pub fn get_buffer_mut(&mut self, name: &str) -> Option<&mut Buffer> {
        let key = Self::normalize_buffer_name(name);
        self.buffers.get_mut(&key)
    }

    /// Normalize a buffer name. IRC channels are case-insensitive,
    /// so we lowercase channel names to avoid duplicate buffers
    /// (e.g., #Rust vs #rust vs #RUST from bouncers).
    pub fn normalize_buffer_name(name: &str) -> String {
        if is_channel(name) {
            name.to_lowercase()
        } else {
            name.to_string()
        }
    }

    pub fn ensure_buffer(&mut self, name: &str) {
        let key = Self::normalize_buffer_name(name);
        if !self.buffers.contains_key(&key) {
            self.buffers.insert(key.clone(), Buffer::new());
            self.buffer_order.push(key);
        }
    }

    /// Add a message to a specific buffer. Creates the buffer if needed.
    /// Increments unread count if the buffer is not the active one.
    pub fn add_message(&mut self, buffer_name: &str, msg: DisplayMessage, scrollback_limit: usize) {
        let key = Self::normalize_buffer_name(buffer_name);
        self.ensure_buffer(&key);
        let is_active = self.active_buffer == key;
        let is_highlight = msg.highlight;
        let buf = self.buffers.get_mut(&key).unwrap();
        buf.messages.push_back(msg);
        while buf.messages.len() > scrollback_limit {
            buf.messages.pop_front();
        }
        if !is_active {
            buf.unread_count += 1;
            if is_highlight {
                buf.highlight_count += 1;
            }
        }
    }

    /// Switch to a buffer by name.
    pub fn switch_buffer(&mut self, name: &str) {
        let key = Self::normalize_buffer_name(name);
        if self.buffers.contains_key(&key) {
            self.active_buffer = key.clone();
            if let Some(buf) = self.buffers.get_mut(&key) {
                buf.unread_count = 0;
                buf.highlight_count = 0;
                buf.scroll_offset = 0;
            }
        }
    }

    /// Return buffer names sorted the same way they display in the sidebar:
    /// server buffer first, then alphabetical (case-insensitive).
    /// When groups are active, group names replace their member channels.
    pub fn sorted_buffers_with_groups(&self, groups: &HashMap<String, BufferGroup>, active_group: Option<&str>) -> Vec<String> {
        // Collect channels that are part of the active group (to exclude individually)
        let grouped_channels: std::collections::HashSet<String> = groups.values()
            .flat_map(|g| g.channels.iter().map(|c| c.to_lowercase()))
            .collect();

        let mut sorted: Vec<String> = self.buffer_order.iter()
            .filter(|b| !grouped_channels.contains(&b.to_lowercase()))
            .cloned()
            .collect();

        // Add group names
        for name in groups.keys() {
            sorted.push(format!("[{}]", name));
        }

        sorted.sort_by(|a, b| {
            if a.is_empty() { return std::cmp::Ordering::Less; }
            if b.is_empty() { return std::cmp::Ordering::Greater; }
            a.to_lowercase().cmp(&b.to_lowercase())
        });
        sorted
    }

    /// Return buffer names sorted (without group logic, used by Alt+num etc).
    pub fn sorted_buffers(&self) -> Vec<String> {
        let mut sorted: Vec<String> = self.buffer_order.clone();
        sorted.sort_by(|a, b| {
            if a.is_empty() { return std::cmp::Ordering::Less; }
            if b.is_empty() { return std::cmp::Ordering::Greater; }
            a.to_lowercase().cmp(&b.to_lowercase())
        });
        sorted
    }

    /// Cycle to next/previous buffer.
    pub fn cycle_buffer(&mut self, forward: bool) {
        if self.buffer_order.len() <= 1 {
            return;
        }
        let current_idx = self
            .buffer_order
            .iter()
            .position(|b| *b == self.active_buffer)
            .unwrap_or(0);
        let new_idx = if forward {
            (current_idx + 1) % self.buffer_order.len()
        } else {
            (current_idx + self.buffer_order.len() - 1) % self.buffer_order.len()
        };
        let name = self.buffer_order[new_idx].clone();
        self.switch_buffer(&name);
    }

    /// Remove a buffer and switch to an adjacent one.
    pub fn remove_buffer(&mut self, name: &str) {
        let key = Self::normalize_buffer_name(name);
        if let Some(pos) = self.buffer_order.iter().position(|b| *b == key) {
            self.buffers.remove(&key);
            self.buffer_order.remove(pos);
            if self.active_buffer == key {
                let new_idx = if pos > 0 { pos - 1 } else { 0 };
                if let Some(new_name) = self.buffer_order.get(new_idx).cloned() {
                    self.switch_buffer(&new_name);
                } else {
                    self.active_buffer = String::new(); // server buffer
                }
            }
        }
    }

    /// Get total unread count across all non-active buffers.
    pub fn total_unread(&self) -> u32 {
        self.buffers
            .iter()
            .filter(|(k, _)| **k != self.active_buffer)
            .map(|(_, v)| v.unread_count)
            .sum()
    }

    /// Get total highlight count across all non-active buffers.
    pub fn total_highlights(&self) -> u32 {
        self.buffers
            .iter()
            .filter(|(k, _)| **k != self.active_buffer)
            .map(|(_, v)| v.highlight_count)
            .sum()
    }
}

/// Main application state supporting multiple servers.
pub struct App {
    pub servers: HashMap<String, ServerState>,
    pub active_server: Option<String>,
    pub server_order: Vec<String>,
    // Global state
    pub input: String,
    pub cursor_pos: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub should_quit: bool,
    pub scrollback_limit: usize,
    pub timestamp_format: String,
    pub input_mode: InputMode,
    pub vault_unlocked: bool,
    pub irc_config: IrcConfig,
    pub connect_to: Option<String>,
    /// Theme switch request from /theme command.
    pub theme_switch: Option<String>,
    /// Currently active theme name.
    pub active_theme: String,
    /// Tab completion state.
    pub tab_state: Option<TabCompletionState>,
    /// Notification configuration.
    pub notification_config: NotificationConfig,
    /// Command to open URLs (e.g., "open" on macOS).
    pub url_open_command: String,
    /// Show join/part/quit messages.
    pub show_join_part: bool,
    /// Show user@host in join messages.
    pub show_hostmask_on_join: bool,
    /// Configurable display format strings.
    pub formats: FormatsConfig,
    /// Compiled snotice regex rules.
    pub snotice_rules: Vec<CompiledSnoticeRule>,
    /// Raw snotice rule configs (for add/remove/save).
    pub snotice_configs: Vec<flume_core::config::formats::SnoticeRuleConfig>,
    /// Active keybinding mode.
    pub keybinding_mode: KeybindingMode,
    /// Vi sub-mode (Normal/Insert). Only used when keybinding_mode == Vi.
    pub vi_mode: ViMode,
    /// Vi operator-pending key (e.g., 'd' waiting for second 'd' in 'dd').
    pub vi_pending_op: Option<char>,
    /// Active split state (None = single buffer view).
    pub split: Option<SplitState>,
    /// Pending script command (processed in main loop which owns the ScriptManager).
    pub script_command: Option<String>,
    /// Pending LLM generation result for review.
    pub pending_generation: Option<PendingGeneration>,
    /// Request to start an LLM generation (processed in main loop).
    pub generate_request: Option<GenerateRequest>,
    /// True while an LLM generation is in flight.
    pub generating: bool,
    /// True when viewing the global flume buffer (/go flume).
    pub viewing_global: bool,
    /// True until user submits first input or a server connects.
    pub show_splash: bool,
    /// Interactive /generate init step (None = not in init flow).
    pub generate_init_step: Option<u8>,
    /// Active DCC transfers.
    pub dcc_transfers: Vec<DccTransfer>,
    /// Pending DCC command (processed in main loop).
    pub dcc_command: Option<String>,
    /// Channels for sending messages to DCC CHAT sessions (id → tx).
    pub dcc_chat_senders: HashMap<u64, tokio::sync::mpsc::Sender<String>>,
    /// User-defined color combos (runtime copy, persisted via /save).
    pub combos: std::collections::HashMap<String, flume_core::config::combos::ComboDefinition>,
    /// Buffer groups: named pairs of channels shown as a single buffer entry.
    pub groups: HashMap<String, BufferGroup>,
    /// Name of the currently active group (if viewing one).
    pub active_group: Option<String>,
    /// Primary split pane area (for mouse click focus).
    pub primary_pane_area: ratatui::layout::Rect,
    /// Secondary split pane area (for mouse click focus).
    pub secondary_pane_area: ratatui::layout::Rect,
    /// User-defined command aliases (runtime copy, persisted via /save).
    pub aliases: std::collections::HashMap<String, String>,
    /// Mouse support enabled.
    pub mouse_enabled: bool,
    /// Flag: mouse state changed, main loop should apply.
    pub mouse_changed: bool,
    /// Buffer list area from last render (for mouse hit testing).
    pub buffer_list_area: ratatui::layout::Rect,
    /// Chat area from last render (for mouse scroll).
    pub chat_area: ratatui::layout::Rect,
    /// Last /url listing (cached for /url open <n>).
    pub last_url_list: Vec<String>,
    // Global buffer for messages when no server is active
    global_messages: VecDeque<DisplayMessage>,
}

/// A buffer group: two channels displayed as a single buffer entry.
#[derive(Clone)]
pub struct BufferGroup {
    pub server: String,
    pub channels: [String; 2],
    pub ratio: u16,
    pub direction: crate::split::SplitDirection,
}

/// State for nick tab-completion cycling.
pub struct TabCompletionState {
    /// The partial text being completed.
    pub prefix: String,
    /// Position in input where the word starts.
    pub word_start: usize,
    /// Matching nicks.
    pub matches: Vec<String>,
    /// Current match index.
    pub index: usize,
}

impl App {
    pub fn new(
        scrollback_limit: usize,
        timestamp_format: &str,
        notification_config: NotificationConfig,
        url_open_command: String,
        keybinding_mode: KeybindingMode,
        show_join_part: bool,
        show_hostmask_on_join: bool,
        formats: FormatsConfig,
        combos: std::collections::HashMap<String, flume_core::config::combos::ComboDefinition>,
        aliases: std::collections::HashMap<String, String>,
        mouse_enabled: bool,
        groups: HashMap<String, BufferGroup>,
    ) -> Self {
        // Load snotice rules from file, merge with any in [formats] config
        let mut snotice_configs = flume_core::config::load_snotice_rules();
        snotice_configs.extend(formats.snotice.clone());
        let snotice_rules = compile_snotice_rules(&snotice_configs);
        App {
            servers: HashMap::new(),
            active_server: None,
            server_order: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            input_history: Vec::new(),
            history_index: None,
            should_quit: false,
            scrollback_limit,
            timestamp_format: timestamp_format.to_string(),
            input_mode: InputMode::Normal,
            vault_unlocked: false,
            irc_config: IrcConfig::default(),
            connect_to: None,
            theme_switch: None,
            active_theme: String::new(),
            tab_state: None,
            notification_config,
            url_open_command,
            show_join_part,
            show_hostmask_on_join,
            formats,
            snotice_rules,
            snotice_configs,
            keybinding_mode,
            vi_mode: ViMode::Insert,
            vi_pending_op: None,
            split: None,
            script_command: None,
            pending_generation: None,
            generate_request: None,
            generating: false,
            viewing_global: false,
            show_splash: true,
            generate_init_step: None,
            dcc_transfers: Vec::new(),
            dcc_command: None,
            dcc_chat_senders: HashMap::new(),
            combos,
            aliases,
            mouse_enabled,
            mouse_changed: false,
            groups,
            active_group: None,
            primary_pane_area: ratatui::layout::Rect::default(),
            secondary_pane_area: ratatui::layout::Rect::default(),
            buffer_list_area: ratatui::layout::Rect::default(),
            chat_area: ratatui::layout::Rect::default(),
            last_url_list: Vec::new(),
            global_messages: VecDeque::new(),
        }
    }

    // --- Server management ---

    pub fn add_server(&mut self, name: &str, nick: &str) {
        if !self.servers.contains_key(name) {
            self.servers
                .insert(name.to_string(), ServerState::new(name, nick));
            self.server_order.push(name.to_string());
            // If no active server, make this one active
            if self.active_server.is_none() {
                self.active_server = Some(name.to_string());
            }
        }
    }

    pub fn switch_server(&mut self, name: &str) {
        if self.servers.contains_key(name) {
            self.active_server = Some(name.to_string());
        }
    }

    /// Get groups for the active server only.
    pub fn active_groups(&self) -> HashMap<String, BufferGroup> {
        let server = self.active_server.as_deref().unwrap_or("");
        self.groups.iter()
            .filter(|(_, g)| g.server == server)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Switch to a buffer group — activates split with both channels.
    pub fn switch_to_group(&mut self, group_name: &str) {
        let group = match self.groups.get(group_name) {
            Some(g) => g.clone(),
            None => return,
        };
        // Leave any current group first
        self.leave_group();

        let server = self.active_server.clone().unwrap_or_default();
        if let Some(ss) = self.servers.get_mut(&server) {
            ss.switch_buffer(&group.channels[0]);
        }
        self.split = Some(crate::split::SplitState::new(
            group.direction.clone(),
            server,
            group.channels[1].clone(),
        ));
        // Set the ratio
        if let Some(ref mut s) = self.split {
            s.ratio = group.ratio;
        }
        self.active_group = Some(group_name.to_string());
    }

    /// Leave the current group — clears the split.
    pub fn leave_group(&mut self) {
        if self.active_group.is_some() {
            self.split = None;
            self.active_group = None;
        }
    }

    pub fn cycle_server(&mut self) {
        if self.server_order.len() <= 1 {
            return;
        }
        let current_idx = self
            .active_server
            .as_ref()
            .and_then(|s| self.server_order.iter().position(|n| n == s))
            .unwrap_or(0);
        let new_idx = (current_idx + 1) % self.server_order.len();
        self.active_server = Some(self.server_order[new_idx].clone());
    }

    // --- Accessors for active server/buffer ---

    pub fn active_server_state(&self) -> Option<&ServerState> {
        self.active_server
            .as_ref()
            .and_then(|s| self.servers.get(s))
    }

    pub fn active_server_state_mut(&mut self) -> Option<&mut ServerState> {
        self.active_server
            .as_ref()
            .cloned()
            .and_then(move |s| self.servers.get_mut(&s))
    }

    pub fn active_messages(&self) -> &VecDeque<DisplayMessage> {
        if self.viewing_global {
            return &self.global_messages;
        }
        self.active_server_state()
            .map(|s| &s.active_buf().messages)
            .unwrap_or(&self.global_messages)
    }

    pub fn active_nicks(&self) -> &[ChannelNick] {
        self.active_server_state()
            .map(|s| s.active_buf().nicks.as_slice())
            .unwrap_or(&[])
    }

    pub fn active_scroll_offset(&self) -> usize {
        self.active_server_state()
            .map(|s| s.active_buf().scroll_offset)
            .unwrap_or(0)
    }

    pub fn active_nick(&self) -> &str {
        self.active_server_state()
            .map(|s| s.nick.as_str())
            .unwrap_or("flume")
    }

    pub fn active_server_name(&self) -> &str {
        self.active_server
            .as_deref()
            .unwrap_or("Flume")
    }

    /// Returns the active buffer name if it's a channel or query, None if server buffer.
    pub fn active_target(&self) -> Option<&str> {
        self.active_server_state().and_then(|s| {
            if s.active_buffer.is_empty() {
                None
            } else {
                Some(s.active_buffer.as_str())
            }
        })
    }

    pub fn active_connection_state(&self) -> ConnectionState {
        self.active_server_state()
            .map(|s| s.connection_state.clone())
            .unwrap_or(ConnectionState::Disconnected)
    }

    pub fn active_command_tx(&self) -> Option<&mpsc::Sender<UserCommand>> {
        self.active_server_state()
            .and_then(|s| s.command_tx.as_ref())
    }

    // --- Scrolling (delegates to active buffer) ---

    pub fn scroll_up(&mut self, amount: usize) {
        if let Some(ss) = self.active_server_state_mut() {
            let buf = ss.active_buf_mut();
            let max_scroll = buf.messages.len().saturating_sub(1);
            buf.scroll_offset = (buf.scroll_offset + amount).min(max_scroll);
        }
    }

    pub fn scroll_down(&mut self, amount: usize) {
        if let Some(ss) = self.active_server_state_mut() {
            let buf = ss.active_buf_mut();
            buf.scroll_offset = buf.scroll_offset.saturating_sub(amount);
        }
    }

    // --- Split management ---

    /// Create a split showing a secondary buffer.
    #[allow(dead_code)]
    pub fn split_buffer(
        &mut self,
        direction: SplitDirection,
        server: String,
        buffer: String,
    ) {
        // Verify the server and buffer exist
        if let Some(ss) = self.servers.get(&server) {
            let buffer_key = ServerState::normalize_buffer_name(&buffer);
            if ss.buffers.contains_key(&buffer_key) {
                self.split = Some(SplitState::new(direction, server, buffer));
            }
        }
    }

    /// Remove the split, returning to single-buffer view.
    pub fn unsplit(&mut self) {
        self.split = None;
    }

    /// Swap focus to the other pane. The secondary buffer becomes active,
    /// and the previously active buffer becomes the secondary.
    pub fn swap_split_focus(&mut self) {
        let Some(ref mut split) = self.split else {
            return;
        };
        let Some(ref active_server) = self.active_server.clone() else {
            return;
        };
        let Some(ss) = self.servers.get(active_server) else {
            return;
        };
        let old_active_buffer = ss.active_buffer.clone();
        let old_active_server = active_server.clone();
        let new_server = split.secondary_server.clone();
        let new_buffer = split.secondary_buffer.clone();

        // Update split to point to what was the active buffer
        split.secondary_server = old_active_server;
        split.secondary_buffer = old_active_buffer;

        // Switch to the new server+buffer
        self.active_server = Some(new_server.clone());
        if let Some(ss) = self.servers.get_mut(&new_server) {
            ss.switch_buffer(&new_buffer);
        }
    }

    /// Get messages for the secondary (split) pane.
    pub fn split_messages(&self) -> Option<&VecDeque<DisplayMessage>> {
        let split = self.split.as_ref()?;
        let ss = self.servers.get(&split.secondary_server)?;
        ss.buffers.get(&split.secondary_buffer).map(|b| &b.messages)
    }

    /// Get scroll offset for the secondary (split) pane.
    pub fn split_scroll_offset(&self) -> usize {
        self.split
            .as_ref()
            .and_then(|s| self.servers.get(&s.secondary_server))
            .and_then(|ss| ss.buffers.get(&ss.active_buffer))
            .map(|b| b.scroll_offset)
            .unwrap_or(0)
    }

    /// Get search pattern for the secondary (split) pane.
    pub fn split_search(&self) -> Option<&str> {
        let split = self.split.as_ref()?;
        let ss = self.servers.get(&split.secondary_server)?;
        ss.buffers.get(&split.secondary_buffer)?.search.as_deref()
    }

    pub fn split_nicks(&self) -> &[ChannelNick] {
        self.split
            .as_ref()
            .and_then(|s| self.servers.get(&s.secondary_server))
            .and_then(|ss| ss.buffers.get(&ss.active_buffer))
            .map(|b| b.nicks.as_slice())
            .unwrap_or(&[])
    }

    // --- Message helpers ---

    /// Add a system message to the active server's active buffer (or global if no server).
    pub fn system_message(&mut self, text: &str) {
        let msg = DisplayMessage {
            timestamp: chrono::Utc::now(),
            source: MessageSource::System,
            text: text.to_string(),
            highlight: false,
        };

        // Always log to the global flume buffer
        self.global_messages.push_back(msg.clone());
        while self.global_messages.len() > self.scrollback_limit {
            self.global_messages.pop_front();
        }

        // Also show in the active buffer for immediate visibility
        if let Some(ref server_name) = self.active_server.clone() {
            if let Some(ss) = self.servers.get_mut(server_name) {
                let active_buf = ss.active_buffer.clone();
                ss.add_message(&active_buf, msg, self.scrollback_limit);
            }
        }
    }

    /// Add a system message to a specific server's server buffer ("").
    pub fn system_message_to(&mut self, server_name: &str, text: &str) {
        let msg = DisplayMessage {
            timestamp: chrono::Utc::now(),
            source: MessageSource::System,
            text: text.to_string(),
            highlight: false,
        };
        if let Some(ss) = self.servers.get_mut(server_name) {
            ss.add_message("", msg, self.scrollback_limit);
        }
    }

    // --- Input history ---

    pub fn submit_input(&mut self) -> Option<String> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        if self.input_history.last().map(|s| s.as_str()) != Some(&text) {
            self.input_history.push(text.clone());
        }
        self.history_index = None;
        self.input.clear();
        self.cursor_pos = 0;
        self.show_splash = false;
        Some(text)
    }

    pub fn history_up(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        let new_idx = match self.history_index {
            None => self.input_history.len() - 1,
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.history_index = Some(new_idx);
        self.input = self.input_history[new_idx].clone();
        self.cursor_pos = self.input.len();
    }

    pub fn history_down(&mut self) {
        match self.history_index {
            None => return,
            Some(i) => {
                if i + 1 >= self.input_history.len() {
                    self.history_index = None;
                    self.input.clear();
                    self.cursor_pos = 0;
                } else {
                    self.history_index = Some(i + 1);
                    self.input = self.input_history[i + 1].clone();
                    self.cursor_pos = self.input.len();
                }
            }
        }
    }

    // --- IRC event handling ---

    pub fn handle_irc_event(&mut self, event: &IrcEvent) -> Vec<NotificationEvent> {
        let mut notifications = Vec::new();
        let server_name = match event {
            IrcEvent::Connected { server_name, .. }
            | IrcEvent::Disconnected { server_name, .. }
            | IrcEvent::MessageReceived { server_name, .. }
            | IrcEvent::StateChanged { server_name, .. }
            | IrcEvent::Error { server_name, .. } => server_name.clone(),
        };

        // Ensure server exists in our state
        if !self.servers.contains_key(&server_name) {
            return notifications;
        }

        let scrollback = self.scrollback_limit;
        let highlight_words = self.notification_config.highlight_words.clone();

        match event {
            IrcEvent::Connected { our_nick, capabilities, .. } => {
                let ss = self.servers.get_mut(&server_name).unwrap();
                ss.nick = our_nick.clone();
                ss.connection_state = ConnectionState::Connected;
                ss.has_echo_message = capabilities.contains("echo-message");
                self.show_splash = false;
                let msg = DisplayMessage {
                    timestamp: chrono::Utc::now(),
                    source: MessageSource::System,
                    text: format!("Connected as {}", our_nick),
                    highlight: false,
                };
                ss.add_message("", msg, scrollback);
            }
            IrcEvent::Disconnected { reason, .. } => {
                let ss = self.servers.get_mut(&server_name).unwrap();
                ss.connection_state = ConnectionState::Disconnected;
                let msg = DisplayMessage {
                    timestamp: chrono::Utc::now(),
                    source: MessageSource::System,
                    text: format!("Disconnected: {:?}", reason),
                    highlight: false,
                };
                ss.add_message("", msg, scrollback);
            }
            IrcEvent::StateChanged { state, .. } => {
                let ss = self.servers.get_mut(&server_name).unwrap();
                ss.connection_state = state.clone();
                // Show connection progress to the user
                let status_text = match state {
                    ConnectionState::Connecting => format!("Connecting to {}...", server_name),
                    ConnectionState::Registering => format!("Registering on {}...", server_name),
                    ConnectionState::Connected => format!("Connected to {}", server_name),
                    ConnectionState::Disconnected => format!("Disconnected from {}", server_name),
                };
                ss.add_message(
                    "",
                    DisplayMessage {
                        timestamp: chrono::Utc::now(),
                        source: MessageSource::System,
                        text: status_text,
                        highlight: false,
                    },
                    self.scrollback_limit,
                );
            }
            IrcEvent::Error { error, .. } => {
                let msg = DisplayMessage {
                    timestamp: chrono::Utc::now(),
                    source: MessageSource::System,
                    text: format!("Error: {}", error),
                    highlight: false,
                };
                self.servers
                    .get_mut(&server_name)
                    .unwrap()
                    .add_message("", msg, scrollback);
            }
            IrcEvent::MessageReceived { message, .. } => {
                let timestamp = message.server_time.unwrap_or_else(chrono::Utc::now);
                let ss = self.servers.get_mut(&server_name).unwrap();
                let our_nick = ss.nick.clone();

                match &message.command {
                    Command::Privmsg { target, text } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        let is_own = nick == our_nick;

                        // CTCP ACTION
                        if text.starts_with('\x01') && text.ends_with('\x01') {
                            let inner = &text[1..text.len() - 1];
                            if let Some(action_text) = inner.strip_prefix("ACTION ") {
                                let buffer_name = if is_channel(target) {
                                    target.clone()
                                } else if is_own {
                                    target.clone()
                                } else {
                                    nick.to_string()
                                };
                                let is_pm = !is_channel(&buffer_name) && !is_own;
                                let highlight = !is_own
                                    && (is_pm || is_highlight(action_text, &our_nick, &highlight_words));
                                if highlight {
                                    if is_pm {
                                        notifications.push(NotificationEvent::PrivateMessage {
                                            server: server_name.clone(),
                                            nick: nick.to_string(),
                                            text: action_text.to_string(),
                                        });
                                    } else {
                                        notifications.push(NotificationEvent::Highlight {
                                            server: server_name.clone(),
                                            buffer: buffer_name.clone(),
                                            nick: nick.to_string(),
                                            text: action_text.to_string(),
                                        });
                                    }
                                }
                                ss.add_message(
                                    &buffer_name,
                                    DisplayMessage {
                                        timestamp,
                                        source: MessageSource::Action(nick.to_string()),
                                        text: action_text.to_string(),
                                        highlight,
                                    },
                                    scrollback,
                                );
                                return notifications;
                            }
                            // Non-ACTION CTCP — display as system message
                            let ctcp_cmd = inner.split(' ').next().unwrap_or(inner);
                            let ctcp_params = inner.strip_prefix(ctcp_cmd).unwrap_or("").trim();
                            ss.add_message(
                                "",
                                DisplayMessage {
                                    timestamp,
                                    source: MessageSource::Server,
                                    text: if ctcp_params.is_empty() {
                                        format!("[ctcp] {} from {}", ctcp_cmd, nick)
                                    } else {
                                        format!("[ctcp] {} from {}: {}", ctcp_cmd, nick, ctcp_params)
                                    },
                                    highlight: false,
                                },
                                scrollback,
                            );
                            return notifications;
                        }

                        // Deduplicate echo-message: skip if we sent this exact
                        // message recently (within 30s). Allows bouncer playback
                        // of older own messages to come through.
                        if is_own && ss.has_echo_message {
                            let now = chrono::Utc::now();
                            let is_recent_echo = ss.recent_own_messages.iter().any(|(msg_text, sent_at)| {
                                msg_text == text && (now - *sent_at).num_seconds() < 30
                            });
                            if is_recent_echo {
                                tracing::trace!("Echo dedup: skipping own message");
                                ss.recent_own_messages.retain(|(msg_text, _)| msg_text != text);
                                return notifications;
                            }
                        }

                        let source = if is_own {
                            MessageSource::Own(nick.to_string())
                        } else {
                            MessageSource::User(nick.to_string())
                        };

                        // Route to channel or PM buffer
                        let buffer_name = if is_channel(target) {
                            target.clone()
                        } else if is_own {
                            // Our own PM to someone (echo-message)
                            target.clone()
                        } else {
                            // PM from someone else
                            nick.to_string()
                        };

                        let is_pm = !is_channel(&buffer_name) && !is_own;
                        let highlight =
                            !is_own && (is_pm || is_highlight(text, &our_nick, &highlight_words));
                        if highlight {
                            if is_pm {
                                notifications.push(NotificationEvent::PrivateMessage {
                                    server: server_name.clone(),
                                    nick: nick.to_string(),
                                    text: text.clone(),
                                });
                            } else {
                                notifications.push(NotificationEvent::Highlight {
                                    server: server_name.clone(),
                                    buffer: buffer_name.clone(),
                                    nick: nick.to_string(),
                                    text: text.clone(),
                                });
                            }
                        }

                        ss.add_message(
                            &buffer_name,
                            DisplayMessage {
                                timestamp,
                                source,
                                text: text.clone(),
                                highlight,
                            },
                            scrollback,
                        );
                    }
                    Command::Join { channels } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        let userhost = message.prefix_userhost();
                        if let Some((channel, _)) = channels.first() {
                            if nick == our_nick {
                                ss.ensure_buffer(channel);
                                ss.switch_buffer(channel);
                                ss.add_message(
                                    channel,
                                    DisplayMessage {
                                        timestamp,
                                        source: MessageSource::System,
                                        text: format!("Joined {}", channel),
                                        highlight: false,
                                    },
                                    scrollback,
                                );
                            } else {
                                if let Some(buf) = ss.get_buffer_mut(channel) {
                                    buf.add_nick("", nick);
                                }
                                if self.show_join_part {
                                    let uh = userhost.as_deref().unwrap_or("");
                                    let vars = fmt_vars!(
                                        "nick" => nick,
                                        "userhost" => uh,
                                        "channel" => channel.as_str()
                                    );
                                    let text = format_string(&self.formats.join, &vars);
                                    ss.add_message(
                                        channel,
                                        DisplayMessage {
                                            timestamp,
                                            source: MessageSource::System,
                                            text,
                                            highlight: false,
                                        },
                                        scrollback,
                                    );
                                }
                            }
                        }
                    }
                    Command::Part { channels, message: part_msg } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        let is_self = nick == ss.nick;
                        for channel in channels {
                            if let Some(buf) = ss.get_buffer_mut(channel) {
                                buf.remove_nick(nick);
                            }
                            if self.show_join_part || is_self {
                                let msg_str = part_msg.as_deref().unwrap_or("");
                                let vars = fmt_vars!(
                                    "nick" => nick,
                                    "channel" => channel.as_str(),
                                    "message" => msg_str
                                );
                                let text = format_string(&self.formats.part, &vars);
                                ss.add_message(
                                    channel,
                                    DisplayMessage {
                                        timestamp,
                                        source: MessageSource::System,
                                        text,
                                        highlight: false,
                                    },
                                    scrollback,
                                );
                            }
                            if is_self {
                                ss.remove_buffer(channel);
                            }
                        }
                    }
                    Command::Quit { message: quit_msg } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        // Find channels where this nick was present, remove them, and notify
                        let mut channels_with_nick: Vec<String> = Vec::new();
                        for (buf_name, buf) in &ss.buffers {
                            if is_channel(buf_name) && buf.nicks.iter().any(|cn| cn.nick == nick) {
                                channels_with_nick.push(buf_name.clone());
                            }
                        }
                        for buf_name in &channels_with_nick {
                            if let Some(buf) = ss.get_buffer_mut(buf_name) {
                                buf.remove_nick(nick);
                            }
                        }
                        if self.show_join_part && !channels_with_nick.is_empty() {
                            let msg_str = quit_msg.as_deref().unwrap_or("");
                            let vars = fmt_vars!("nick" => nick, "message" => msg_str);
                            let text = format_string(&self.formats.quit, &vars);
                            let msg = DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text,
                                highlight: false,
                            };
                            for buf_name in &channels_with_nick {
                                ss.add_message(buf_name, msg.clone(), scrollback);
                            }
                        }
                    }
                    Command::Nick { nickname } => {
                        let old_nick = message.prefix_nick().unwrap_or("???");
                        if old_nick == our_nick {
                            ss.nick = nickname.clone();
                        }
                        // Find channels where this nick is present
                        let channels_with_nick: Vec<String> = ss
                            .buffers
                            .iter()
                            .filter(|(k, buf)| {
                                is_channel(k) && buf.nicks.iter().any(|cn| cn.nick == old_nick)
                            })
                            .map(|(k, _)| k.clone())
                            .collect();
                        // Rename and notify only in those channels
                        for buf_name in &channels_with_nick {
                            if let Some(buf) = ss.get_buffer_mut(buf_name) {
                                buf.rename_nick(old_nick, nickname);
                            }
                        }
                        if self.show_join_part && !channels_with_nick.is_empty() {
                            let vars = fmt_vars!(
                                "old_nick" => old_nick,
                                "new_nick" => nickname.as_str()
                            );
                            let text = format_string(&self.formats.nick_change, &vars);
                            let msg = DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text,
                                highlight: false,
                            };
                            for buf_name in &channels_with_nick {
                                ss.add_message(buf_name, msg.clone(), scrollback);
                            }
                        }
                    }
                    Command::Topic { channel, topic } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        if let Some(buf) = ss.get_buffer_mut(channel) {
                            buf.topic = topic.clone();
                        }
                        let topic_str = topic.as_deref().unwrap_or("");
                        let vars = fmt_vars!(
                            "nick" => nick,
                            "channel" => channel.as_str(),
                            "topic" => topic_str
                        );
                        let text = format_string(&self.formats.topic, &vars);
                        ss.add_message(
                            channel,
                            DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text,
                                highlight: false,
                            },
                            scrollback,
                        );
                    }
                    Command::Kick { channel, user, reason } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        // Remove kicked user from nick list
                        if let Some(buf) = ss.get_buffer_mut(channel) {
                            buf.remove_nick(user);
                        }
                        let reason_str = reason.as_deref().unwrap_or("");
                        let vars = fmt_vars!(
                            "nick" => nick,
                            "target" => user.as_str(),
                            "channel" => channel.as_str(),
                            "reason" => reason_str
                        );
                        let text = format_string(&self.formats.kick, &vars);
                        ss.add_message(
                            channel,
                            DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text,
                                highlight: false,
                            },
                            scrollback,
                        );
                    }
                    Command::Numeric { code, params } => {
                        // Helper: labeled server message to server buffer
                        let labeled = |label: &str, text: &str| DisplayMessage {
                            timestamp,
                            source: MessageSource::Server,
                            text: format!("[{}] {}", label, text),
                            highlight: false,
                        };
                        // Helper: labeled server message to a specific buffer
                        let labeled_sys = |text: String| DisplayMessage {
                            timestamp,
                            source: MessageSource::System,
                            text,
                            highlight: false,
                        };
                        // Shorthand for last param
                        let last = || params.last().cloned().unwrap_or_default();
                        let p = |i: usize| params.get(i).cloned().unwrap_or_default();

                        match *code {
                            // === Connection/Welcome ===
                            1 => {
                                // RPL_WELCOME
                                ss.add_message("", labeled("welcome", &last()), scrollback);
                            }
                            2 => {
                                // RPL_YOURHOST
                                ss.add_message("", labeled("host", &last()), scrollback);
                            }
                            3 => {
                                // RPL_CREATED
                                ss.add_message("", labeled("created", &last()), scrollback);
                            }
                            4 => {
                                // RPL_MYINFO: server version usermodes chanmodes
                                let srv = p(1);
                                let ver = p(2);
                                ss.add_message("", labeled("server", &format!("{} ({})", srv, ver)), scrollback);
                            }
                            5 => {
                                // RPL_ISUPPORT — silently consumed (tokens like CHANTYPES, PREFIX, etc.)
                                // Servers send multiple 005s; not useful to display
                            }

                            // === MOTD ===
                            375 => {
                                // RPL_MOTDSTART
                                ss.add_message("", labeled("motd", &last()), scrollback);
                            }
                            372 => {
                                // RPL_MOTD — strip leading "- " prefix that servers add
                                let line = last();
                                let clean = line.strip_prefix("- ").unwrap_or(&line);
                                ss.add_message("", labeled("motd", clean), scrollback);
                            }
                            376 | 422 => {
                                // RPL_ENDOFMOTD / ERR_NOMOTD — silenced
                            }

                            // === LUSERS (server stats) ===
                            251..=255 => {
                                ss.add_message("", labeled("stats", &last()), scrollback);
                            }
                            265 | 266 => {
                                // Local/global users count
                                ss.add_message("", labeled("stats", &last()), scrollback);
                            }

                            // === AWAY status ===
                            301 => {
                                // RPL_AWAY: nick :away message (also in WHOIS)
                                let nick = p(1);
                                let msg = p(2);
                                ss.add_message("", labeled("away", &format!("{} — {}", nick, msg)), scrollback);
                            }
                            305 => {
                                // RPL_UNAWAY
                                ss.add_message("", labeled("away", "You are no longer marked as away"), scrollback);
                            }
                            306 => {
                                // RPL_NOWAWAY
                                ss.add_message("", labeled("away", "You are now marked as away"), scrollback);
                            }

                            // === WHOIS ===
                            311 => {
                                let nick = p(1);
                                let user = p(2);
                                let host = p(3);
                                let realname = p(5);
                                ss.add_message("", labeled("whois",
                                    &format!("{} ({}@{}) — {}", nick, user, host, realname)), scrollback);
                            }
                            312 => {
                                let server = p(2);
                                let info = p(3);
                                ss.add_message("", labeled("server", &format!("{} — {}", server, info)), scrollback);
                            }
                            313 => {
                                ss.add_message("", labeled("oper", &last()), scrollback);
                            }
                            317 => {
                                let idle_secs: u64 = p(2).parse().unwrap_or(0);
                                let idle_str = if idle_secs >= 3600 {
                                    format!("{}h {}m", idle_secs / 3600, (idle_secs % 3600) / 60)
                                } else if idle_secs >= 60 {
                                    format!("{}m {}s", idle_secs / 60, idle_secs % 60)
                                } else {
                                    format!("{}s", idle_secs)
                                };
                                ss.add_message("", labeled("idle", &idle_str), scrollback);
                            }
                            318 | 369 => {
                                // RPL_ENDOFWHOIS / RPL_ENDOFWHOWAS — silenced
                            }
                            319 => {
                                ss.add_message("", labeled("channels", &p(2)), scrollback);
                            }
                            330 => {
                                ss.add_message("", labeled("account", &p(2)), scrollback);
                            }
                            378 => {
                                ss.add_message("", labeled("host", &last()), scrollback);
                            }
                            671 => {
                                ss.add_message("", labeled("secure", "using TLS"), scrollback);
                            }

                            // === USER modes ===
                            221 => {
                                let modes = p(1);
                                ss.user_modes = modes;
                            }

                            // === TOPIC ===
                            332 => {
                                let channel = p(1);
                                let topic = p(2);
                                if let Some(buf) = ss.buffers.get_mut(&channel) {
                                    buf.topic = Some(topic.clone());
                                }
                                ss.add_message(&channel, labeled_sys(format!("Topic: {}", topic)), scrollback);
                            }
                            333 => {
                                // RPL_TOPICWHOTIME: channel nick!user@host timestamp
                                let channel = p(1);
                                let setter = p(2);
                                let setter_nick = setter.split('!').next().unwrap_or(&setter);
                                let ts: i64 = p(3).parse().unwrap_or(0);
                                let time_str = if ts > 0 {
                                    chrono::DateTime::from_timestamp(ts, 0)
                                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };
                                ss.add_message(&channel,
                                    labeled_sys(format!("Set by {} on {}", setter_nick, time_str)), scrollback);
                            }

                            // === CHANNEL MODE ===
                            324 => {
                                // RPL_CHANNELMODEIS: channel modes [params]
                                let channel = p(1);
                                let modes = params[2..].join(" ");
                                if let Some(buf) = ss.buffers.get_mut(&channel) {
                                    buf.channel_modes = Some(modes.clone());
                                }
                                ss.add_message(&channel,
                                    labeled_sys(format!("Channel modes: {}", modes)), scrollback);
                            }
                            329 => {
                                // RPL_CREATIONTIME: channel timestamp
                                let channel = p(1);
                                let ts: i64 = p(2).parse().unwrap_or(0);
                                if ts > 0 {
                                    if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                                        ss.add_message(&channel,
                                            labeled_sys(format!("Created: {}", dt.format("%Y-%m-%d %H:%M"))),
                                            scrollback);
                                    }
                                }
                            }

                            // === NAMES ===
                            353 => {
                                let channel = p(2);
                                let nicks_str = p(3);
                                ss.ensure_buffer(&channel);
                                if let Some(buf) = ss.buffers.get_mut(&channel) {
                                    for entry in nicks_str.split_whitespace() {
                                        let (prefix, nick) = parse_nick_prefix(entry);
                                        buf.add_nick(&prefix, &nick);
                                    }
                                }
                            }
                            366 => {} // RPL_ENDOFNAMES

                            // === WHO ===
                            352 => {
                                // RPL_WHOREPLY: channel user host server nick H|G[@+] :hopcount realname
                                let channel = p(1);
                                let user = p(2);
                                let host = p(3);
                                let nick = p(5);
                                let flags = p(6);
                                let realname = last();
                                // Strip hopcount from realname
                                let rn = realname.split_once(' ').map(|(_, r)| r).unwrap_or(&realname);
                                ss.add_message(&channel, labeled("who",
                                    &format!("{} ({}@{}) [{}] {}", nick, user, host, flags, rn)), scrollback);
                            }
                            315 => {} // RPL_ENDOFWHO

                            // === LIST ===
                            321 => {} // RPL_LISTSTART — silenced
                            322 => {
                                // RPL_LIST: channel users :topic
                                let channel = p(1);
                                let users = p(2);
                                let topic = last();
                                ss.add_message("", labeled("list",
                                    &format!("{} ({} users) {}", channel, users, topic)), scrollback);
                            }
                            323 => {} // RPL_LISTEND

                            // === INVITE ===
                            341 => {
                                let nick = p(1);
                                let channel = p(2);
                                ss.add_message("", labeled("invite",
                                    &format!("Inviting {} to {}", nick, channel)), scrollback);
                            }

                            // === BAN LIST ===
                            367 => {
                                // RPL_BANLIST: channel banmask setter timestamp
                                let channel = p(1);
                                let mask = p(2);
                                let setter = p(3).split('!').next().unwrap_or("").to_string();
                                ss.add_message(&channel, labeled("ban",
                                    &format!("{} (by {})", mask, setter)), scrollback);
                            }
                            368 => {} // RPL_ENDOFBANLIST

                            // === VERSION ===
                            351 => {
                                ss.add_message("", labeled("version", &last()), scrollback);
                            }

                            // === INFO ===
                            371 => {
                                let line = last();
                                let clean = line.strip_prefix("- ").unwrap_or(&line);
                                ss.add_message("", labeled("info", clean), scrollback);
                            }
                            374 => {} // RPL_ENDOFINFO

                            // === Error numerics ===
                            401 => {
                                // ERR_NOSUCHNICK
                                ss.add_message("", labeled("error", &format!("{}: no such nick/channel", p(1))), scrollback);
                            }
                            403 => {
                                ss.add_message("", labeled("error", &format!("{}: no such channel", p(1))), scrollback);
                            }
                            404 => {
                                ss.add_message("", labeled("error", &format!("{}: cannot send to channel", p(1))), scrollback);
                            }
                            421 => {
                                ss.add_message("", labeled("error", &format!("unknown command: {}", p(1))), scrollback);
                            }
                            432 => {
                                ss.add_message("", labeled("error", &format!("erroneous nickname: {}", p(1))), scrollback);
                            }
                            433 => {
                                // ERR_NICKNAMEINUSE — critical, needs visibility
                                ss.add_message("", labeled("error",
                                    &format!("nickname '{}' is already in use", p(1))), scrollback);
                            }
                            441 => {
                                ss.add_message("", labeled("error",
                                    &format!("{} is not on {}", p(1), p(2))), scrollback);
                            }
                            442 => {
                                ss.add_message("", labeled("error",
                                    &format!("you're not on {}", p(1))), scrollback);
                            }
                            443 => {
                                ss.add_message("", labeled("error",
                                    &format!("{} is already on {}", p(1), p(2))), scrollback);
                            }
                            461 => {
                                ss.add_message("", labeled("error",
                                    &format!("{}: not enough parameters", p(1))), scrollback);
                            }
                            462 => {
                                ss.add_message("", labeled("error", "already registered"), scrollback);
                            }
                            // ERR_CHANNELISFULL, UNKNOWNMODE, INVITEONLYCHAN, BANNEDFROMCHAN, BADCHANNELKEY
                            471 => {
                                ss.add_message("", labeled("error", &format!("{}: channel is full", p(1))), scrollback);
                            }
                            473 => {
                                ss.add_message("", labeled("error", &format!("{}: invite only", p(1))), scrollback);
                            }
                            474 => {
                                ss.add_message("", labeled("error", &format!("{}: banned from channel", p(1))), scrollback);
                            }
                            475 => {
                                ss.add_message("", labeled("error", &format!("{}: bad channel key", p(1))), scrollback);
                            }
                            482 => {
                                ss.add_message("", labeled("error",
                                    &format!("{}: you're not a channel operator", p(1))), scrollback);
                            }

                            // === Fallback for unhandled numerics ===
                            _ => {
                                let text = last();
                                if !text.is_empty() {
                                    // Error range (400-599) gets [error] label
                                    let label = if *code >= 400 && *code < 600 { "error" } else { "server" };
                                    ss.add_message("", labeled(label, &text), scrollback);
                                }
                            }
                        }
                    }
                    Command::Mode { target, modes, params } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        let mode_str = modes.as_deref().unwrap_or("");
                        let param_str = params.join(" ");

                        // Track user modes when target is our nick
                        if *target == our_nick {
                            if let Some(m) = modes {
                                apply_user_modes(&mut ss.user_modes, m);
                            }
                        }

                        // Update channel nick prefixes for mode changes
                        if is_channel(target) {
                            if let Some(m) = modes {
                                let key = ServerState::normalize_buffer_name(target);
                                if let Some(buf) = ss.buffers.get_mut(&key) {
                                    apply_channel_nick_modes(buf, m, params);
                                }
                            }
                        }

                        let buffer_name = if is_channel(target) {
                            target.clone()
                        } else {
                            String::new()
                        };
                        ss.add_message(
                            &buffer_name,
                            DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text: {
                                    let full_modes = if param_str.is_empty() {
                                        mode_str.to_string()
                                    } else {
                                        format!("{} {}", mode_str, param_str)
                                    };
                                    let vars = fmt_vars!(
                                        "target" => target.as_str(),
                                        "modes" => full_modes.as_str(),
                                        "nick" => nick
                                    );
                                    format_string(&self.formats.mode, &vars)
                                },
                                highlight: false,
                            },
                            scrollback,
                        );
                    }
                    Command::Invite { nickname, channel } => {
                        let from = message.prefix_nick().unwrap_or("???");
                        ss.add_message(
                            "",
                            DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text: format!("{} has invited you to {}", from, channel),
                                highlight: true,
                            },
                            scrollback,
                        );
                    }
                    Command::Notice { target, text } => {
                        let nick = message.prefix_nick().unwrap_or("");

                        // CTCP replies arrive as NOTICE with \x01 wrappers
                        if text.starts_with('\x01') && text.ends_with('\x01') {
                            let inner = &text[1..text.len() - 1];
                            let ctcp_cmd = inner.split(' ').next().unwrap_or(inner);
                            let ctcp_result = inner.strip_prefix(ctcp_cmd).unwrap_or("").trim();
                            let display_text = if ctcp_cmd.eq_ignore_ascii_case("PING") {
                                // Calculate round-trip time from the timestamp
                                if let Ok(sent_ts) = ctcp_result.parse::<i64>() {
                                    let now_ts = chrono::Utc::now().timestamp();
                                    let rtt_secs = now_ts - sent_ts;
                                    if rtt_secs >= 0 {
                                        format!("[ctcp] PING reply from {}: {}s", nick, rtt_secs)
                                    } else {
                                        format!("[ctcp] PING reply from {}: {}", nick, ctcp_result)
                                    }
                                } else {
                                    format!("[ctcp] PING reply from {}: {}", nick, ctcp_result)
                                }
                            } else if ctcp_result.is_empty() {
                                format!("[ctcp] {} reply from {}", ctcp_cmd, nick)
                            } else {
                                format!("[ctcp] {} reply from {}: {}", ctcp_cmd, nick, ctcp_result)
                            };
                            ss.add_message(
                                "",
                                DisplayMessage {
                                    timestamp,
                                    source: MessageSource::Server,
                                    text: display_text,
                                    highlight: false,
                                },
                                scrollback,
                            );
                            return notifications;
                        }

                        // Server notices (from server or no nick) go to server buffer
                        // User notices go to the appropriate buffer
                        if nick.is_empty() || nick.contains('.') || *target == "*" {
                            // Save raw text for /snotice last (per server)
                            ss.last_raw_snotice = Some(text.to_string());
                            // Server notice — check snotice routing rules
                            let mut handled = false;
                            for rule in &self.snotice_rules {
                                if let Some(caps) = rule.regex.captures(text) {
                                    if rule.suppress {
                                        handled = true;
                                        break;
                                    }
                                    let formatted = match &rule.format {
                                        Some(fmt) => format_regex_captures(fmt, &caps),
                                        None => text.to_string(),
                                    };
                                    let buf_name = rule.buffer.as_deref().unwrap_or("");
                                    // Auto-create snotice buffer if needed
                                    if !buf_name.is_empty() {
                                        ss.ensure_buffer(buf_name);
                                    }
                                    ss.add_message(
                                        buf_name,
                                        DisplayMessage {
                                            timestamp,
                                            source: MessageSource::Server,
                                            text: formatted,
                                            highlight: false,
                                        },
                                        scrollback,
                                    );
                                    handled = true;
                                    break;
                                }
                            }
                            if !handled {
                                // Default server notice format
                                let vars = fmt_vars!("text" => text.as_str());
                                let formatted = format_string(&self.formats.server_notice, &vars);
                                ss.add_message(
                                    "",
                                    DisplayMessage {
                                        timestamp,
                                        source: MessageSource::Server,
                                        text: formatted,
                                        highlight: false,
                                    },
                                    scrollback,
                                );
                            }
                        } else {
                            // User notice
                            let buffer = if is_channel(target) {
                                target.clone()
                            } else {
                                String::new()
                            };
                            let vars = fmt_vars!("nick" => nick, "text" => text.as_str(), "target" => target.as_str());
                            let formatted = format_string(&self.formats.notice, &vars);
                            ss.add_message(
                                &buffer,
                                DisplayMessage {
                                    timestamp,
                                    source: MessageSource::Server,
                                    text: formatted,
                                    highlight: false,
                                },
                                scrollback,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        notifications
    }
}

/// Check if a message text should trigger a highlight.
fn is_highlight(text: &str, our_nick: &str, highlight_words: &[String]) -> bool {
    let lower = text.to_lowercase();
    if lower.contains(&our_nick.to_lowercase()) {
        return true;
    }
    for word in highlight_words {
        if lower.contains(&word.to_lowercase()) {
            return true;
        }
    }
    false
}

/// Parse a nick entry from NAMES reply (e.g., "@nick", "+nick", "nick").
fn parse_nick_prefix(entry: &str) -> (String, String) {
    let mut prefix = String::new();
    let mut chars = entry.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == '@' || c == '+' || c == '%' || c == '~' || c == '&' {
            prefix.push(c);
            chars.next();
        } else {
            break;
        }
    }
    let nick: String = chars.collect();
    (prefix, nick)
}

/// Apply a mode string (e.g., "+iw", "-x", "+o-v") to a user modes string.
fn apply_user_modes(current: &mut String, mode_str: &str) {
    let mut adding = true;
    let mut modes: std::collections::HashSet<char> = current
        .chars()
        .filter(|c| *c != '+')
        .collect();

    for c in mode_str.chars() {
        match c {
            '+' => adding = true,
            '-' => adding = false,
            _ => {
                if adding {
                    modes.insert(c);
                } else {
                    modes.remove(&c);
                }
            }
        }
    }

    if modes.is_empty() {
        current.clear();
    } else {
        let mut sorted: Vec<char> = modes.into_iter().collect();
        sorted.sort();
        *current = format!("+{}", sorted.into_iter().collect::<String>());
    }
}

/// Map channel mode letters to their prefix symbols.
fn mode_to_prefix(mode: char) -> Option<char> {
    match mode {
        'o' => Some('@'),
        'v' => Some('+'),
        'h' => Some('%'),
        'q' => Some('~'),
        'a' => Some('&'),
        _ => None,
    }
}

/// Apply channel mode changes (e.g. +o nick, -v nick) to the nick list.
fn apply_channel_nick_modes(buf: &mut Buffer, mode_str: &str, params: &[String]) {
    let mut adding = true;
    let mut param_idx = 0;

    for c in mode_str.chars() {
        match c {
            '+' => adding = true,
            '-' => adding = false,
            _ => {
                if let Some(prefix_char) = mode_to_prefix(c) {
                    if let Some(target_nick) = params.get(param_idx) {
                        if let Some(cn) = buf.nicks.iter_mut().find(|n| n.nick == *target_nick) {
                            if adding {
                                if !cn.prefix.contains(prefix_char) {
                                    cn.prefix.push(prefix_char);
                                }
                            } else {
                                cn.prefix = cn.prefix.replace(prefix_char, "");
                            }
                        }
                        param_idx += 1;
                    }
                } else {
                    // Non-prefix modes that take a parameter (b, k, l, etc.)
                    match c {
                        'b' | 'e' | 'I' | 'k' => { param_idx += 1; }
                        'l' if adding => { param_idx += 1; }
                        _ => {}
                    }
                }
            }
        }
    }

    buf.sort_nicks();
}
