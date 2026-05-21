//! Typed event surface delivered to Swift via the EventCallback trait.

use crate::{buffers::NickEntry, servers::ConnectionState};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum Event {
    // ── lifecycle ──
    Connected {
        server: String,
        nick: String,
        capabilities: Vec<String>,
    },
    Disconnected {
        server: String,
        reason: String,
    },
    StateChanged {
        server: String,
        state: ConnectionState,
    },
    Error {
        server: String,
        message: String,
    },

    // ── chat ──
    /// PRIVMSG to a channel or user. `is_action` is true for CTCP ACTION
    /// (`/me`), with the inner text already unwrapped.
    Message {
        server: String,
        nick: String,
        target: String,
        text: String,
        is_action: bool,
    },
    Notice {
        server: String,
        nick: String,
        target: String,
        text: String,
    },

    // ── membership ──
    Join {
        server: String,
        nick: String,
        channel: String,
    },
    Part {
        server: String,
        nick: String,
        channel: String,
        reason: Option<String>,
    },
    Quit {
        server: String,
        nick: String,
        reason: Option<String>,
    },
    Kick {
        server: String,
        kicker: String,
        target: String,
        channel: String,
        reason: Option<String>,
    },

    // ── identity ──
    NickChanged {
        server: String,
        old_nick: String,
        new_nick: String,
    },

    // ── channel state ──
    TopicChanged {
        server: String,
        channel: String,
        topic: Option<String>,
        setter: Option<String>,
    },
    ModeChanged {
        server: String,
        target: String,
        modes: String,
        params: Vec<String>,
        setter: Option<String>,
    },
    /// Initial nick list for a channel (from RPL_NAMREPLY 353 + 366).
    NamesUpdate {
        server: String,
        channel: String,
        nicks: Vec<NickEntry>,
    },

    Invited {
        server: String,
        channel: String,
        from: String,
    },

    /// A server notice matched an snotice rule. `buffer` is the rule's target
    /// buffer name (None when the rule didn't set one); `suppressed` true
    /// means the notice was dropped.
    SnoticeRouted {
        server: String,
        rule_id: Option<String>,
        buffer: Option<String>,
        original: String,
        formatted: String,
        suppressed: bool,
    },
}

#[uniffi::export(callback_interface)]
pub trait EventCallback: Send + Sync {
    fn on_event(&self, event: Event);
}
