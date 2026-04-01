use std::collections::{HashMap, VecDeque};

use tokio::sync::mpsc;

use flume_core::config::general::NotificationConfig;
use flume_core::config::keybindings::KeybindingMode;
use flume_core::config::IrcConfig;
use flume_core::event::{ConnectionState, IrcEvent, UserCommand};
use flume_core::irc::command::Command;

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
        }
    }

    pub fn active_buf(&self) -> &Buffer {
        self.buffers.get(&self.active_buffer).unwrap()
    }

    pub fn active_buf_mut(&mut self) -> &mut Buffer {
        self.buffers.get_mut(&self.active_buffer).unwrap()
    }

    /// Ensure a buffer exists, creating it if needed.
    pub fn ensure_buffer(&mut self, name: &str) {
        if !self.buffers.contains_key(name) {
            self.buffers.insert(name.to_string(), Buffer::new());
            self.buffer_order.push(name.to_string());
        }
    }

    /// Add a message to a specific buffer. Creates the buffer if needed.
    /// Increments unread count if the buffer is not the active one.
    pub fn add_message(&mut self, buffer_name: &str, msg: DisplayMessage, scrollback_limit: usize) {
        self.ensure_buffer(buffer_name);
        let is_active = self.active_buffer == buffer_name;
        let is_highlight = msg.highlight;
        let buf = self.buffers.get_mut(buffer_name).unwrap();
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
        if self.buffers.contains_key(name) {
            self.active_buffer = name.to_string();
            if let Some(buf) = self.buffers.get_mut(name) {
                buf.unread_count = 0;
                buf.highlight_count = 0;
                buf.scroll_offset = 0;
            }
        }
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
    /// Tab completion state.
    pub tab_state: Option<TabCompletionState>,
    /// Notification configuration.
    pub notification_config: NotificationConfig,
    /// Command to open URLs (e.g., "open" on macOS).
    pub url_open_command: String,
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
    /// Active DCC transfers.
    pub dcc_transfers: Vec<DccTransfer>,
    /// Pending DCC command (processed in main loop).
    pub dcc_command: Option<String>,
    /// Channels for sending messages to DCC CHAT sessions (id → tx).
    pub dcc_chat_senders: HashMap<u64, tokio::sync::mpsc::Sender<String>>,
    // Global buffer for messages when no server is active
    global_messages: VecDeque<DisplayMessage>,
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
    ) -> Self {
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
            tab_state: None,
            notification_config,
            url_open_command,
            keybinding_mode,
            vi_mode: ViMode::Insert,
            vi_pending_op: None,
            split: None,
            script_command: None,
            pending_generation: None,
            generate_request: None,
            generating: false,
            dcc_transfers: Vec::new(),
            dcc_command: None,
            dcc_chat_senders: HashMap::new(),
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
        self.active_server_state()
            .map(|s| &s.active_buf().messages)
            .unwrap_or(&self.global_messages)
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
            if ss.buffers.contains_key(&buffer) {
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

    // --- Message helpers ---

    /// Add a system message to the active server's active buffer (or global if no server).
    pub fn system_message(&mut self, text: &str) {
        let msg = DisplayMessage {
            timestamp: chrono::Utc::now(),
            source: MessageSource::System,
            text: text.to_string(),
            highlight: false,
        };

        if let Some(ref server_name) = self.active_server.clone() {
            if let Some(ss) = self.servers.get_mut(server_name) {
                let active_buf = ss.active_buffer.clone();
                ss.add_message(&active_buf, msg, self.scrollback_limit);
                return;
            }
        }
        // No active server — use global buffer
        self.global_messages.push_back(msg);
        while self.global_messages.len() > self.scrollback_limit {
            self.global_messages.pop_front();
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
            IrcEvent::Connected { our_nick, .. } => {
                let ss = self.servers.get_mut(&server_name).unwrap();
                ss.nick = our_nick.clone();
                ss.connection_state = ConnectionState::Connected;
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
                self.servers.get_mut(&server_name).unwrap().connection_state = state.clone();
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
                                let buffer_name = if target.starts_with('#') {
                                    target.clone()
                                } else if is_own {
                                    target.clone()
                                } else {
                                    nick.to_string()
                                };
                                let is_pm = !buffer_name.starts_with('#') && !is_own;
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
                        }

                        let source = if is_own {
                            MessageSource::Own(nick.to_string())
                        } else {
                            MessageSource::User(nick.to_string())
                        };

                        // Route to channel or PM buffer
                        let buffer_name = if target.starts_with('#') {
                            target.clone()
                        } else if is_own {
                            // Our own PM to someone (echo-message)
                            target.clone()
                        } else {
                            // PM from someone else
                            nick.to_string()
                        };

                        let is_pm = !buffer_name.starts_with('#') && !is_own;
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
                                // Add nick to channel's nick list
                                if let Some(buf) = ss.buffers.get_mut(channel.as_str()) {
                                    buf.add_nick("", nick);
                                }
                                ss.add_message(
                                    channel,
                                    DisplayMessage {
                                        timestamp,
                                        source: MessageSource::System,
                                        text: format!("{} joined {}", nick, channel),
                                        highlight: false,
                                    },
                                    scrollback,
                                );
                            }
                        }
                    }
                    Command::Part { channels, message: part_msg } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        for channel in channels {
                            // Remove nick from channel
                            if let Some(buf) = ss.buffers.get_mut(channel.as_str()) {
                                buf.remove_nick(nick);
                            }
                            let text = match part_msg {
                                Some(m) => format!("{} left {} ({})", nick, channel, m),
                                None => format!("{} left {}", nick, channel),
                            };
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
                    Command::Quit { message: quit_msg } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        let text = match quit_msg {
                            Some(m) => format!("{} quit ({})", nick, m),
                            None => format!("{} quit", nick),
                        };
                        let msg = DisplayMessage {
                            timestamp,
                            source: MessageSource::System,
                            text,
                            highlight: false,
                        };
                        // Remove nick from all channel buffers and post quit message
                        let buffer_names: Vec<String> = ss
                            .buffers
                            .keys()
                            .filter(|k| k.starts_with('#'))
                            .cloned()
                            .collect();
                        for buf_name in &buffer_names {
                            if let Some(buf) = ss.buffers.get_mut(buf_name.as_str()) {
                                buf.remove_nick(nick);
                            }
                            ss.add_message(buf_name, msg.clone(), scrollback);
                        }
                    }
                    Command::Nick { nickname } => {
                        let old_nick = message.prefix_nick().unwrap_or("???");
                        if old_nick == our_nick {
                            ss.nick = nickname.clone();
                        }
                        let msg = DisplayMessage {
                            timestamp,
                            source: MessageSource::System,
                            text: format!("{} is now known as {}", old_nick, nickname),
                            highlight: false,
                        };
                        // Rename nick in all channel buffers
                        let buffer_names: Vec<String> = ss
                            .buffers
                            .keys()
                            .filter(|k| k.starts_with('#'))
                            .cloned()
                            .collect();
                        for buf_name in &buffer_names {
                            if let Some(buf) = ss.buffers.get_mut(buf_name.as_str()) {
                                buf.rename_nick(old_nick, nickname);
                            }
                            ss.add_message(buf_name, msg.clone(), scrollback);
                        }
                    }
                    Command::Topic { channel, topic } => {
                        let nick = message.prefix_nick().unwrap_or("???");
                        let text = match topic {
                            Some(t) => format!("{} set topic of {} to: {}", nick, channel, t),
                            None => format!("{} cleared topic of {}", nick, channel),
                        };
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
                        if let Some(buf) = ss.buffers.get_mut(channel.as_str()) {
                            buf.remove_nick(user);
                        }
                        let text = match reason {
                            Some(r) => format!("{} kicked {} from {} ({})", nick, user, channel, r),
                            None => format!("{} kicked {} from {}", nick, user, channel),
                        };
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
                    Command::Notice { text, .. } => {
                        ss.add_message(
                            "",
                            DisplayMessage {
                                timestamp,
                                source: MessageSource::Server,
                                text: text.clone(),
                                highlight: false,
                            },
                            scrollback,
                        );
                    }
                    Command::Numeric { code, params } => {
                        match *code {
                            // RPL_UMODEIS (221) — our user modes
                            221 => {
                                let modes = params.get(1).cloned().unwrap_or_default();
                                ss.user_modes = modes;
                            }
                            // RPL_TOPIC (332)
                            332 => {
                                let channel = params.get(1).cloned().unwrap_or_default();
                                let topic = params.get(2).cloned().unwrap_or_default();
                                ss.add_message(
                                    &channel,
                                    DisplayMessage {
                                        timestamp,
                                        source: MessageSource::System,
                                        text: format!("Topic: {}", topic),
                                        highlight: false,
                                    },
                                    scrollback,
                                );
                            }
                            // RPL_NAMREPLY (353) — nick list for a channel
                            353 => {
                                // params: [our_nick, "=" or "@" or "*", #channel, "nick1 @nick2 +nick3 ..."]
                                let channel = params.get(2).cloned().unwrap_or_default();
                                let nicks_str = params.get(3).cloned().unwrap_or_default();
                                ss.ensure_buffer(&channel);
                                if let Some(buf) = ss.buffers.get_mut(&channel) {
                                    for entry in nicks_str.split_whitespace() {
                                        let (prefix, nick) = parse_nick_prefix(entry);
                                        buf.add_nick(&prefix, &nick);
                                    }
                                }
                            }
                            // RPL_ENDOFNAMES (366) — end of nick list, nothing to do
                            366 => {}
                            _ => {
                                let text = params.last().cloned().unwrap_or_default();
                                if !text.is_empty() {
                                    ss.add_message(
                                        "",
                                        DisplayMessage {
                                            timestamp,
                                            source: MessageSource::Server,
                                            text,
                                            highlight: false,
                                        },
                                        scrollback,
                                    );
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

                        let buffer_name = if target.starts_with('#') {
                            target.clone()
                        } else {
                            String::new()
                        };
                        ss.add_message(
                            &buffer_name,
                            DisplayMessage {
                                timestamp,
                                source: MessageSource::System,
                                text: format!("{} sets mode {} {} {}", nick, target, mode_str, param_str),
                                highlight: false,
                            },
                            scrollback,
                        );
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
