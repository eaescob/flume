//! Typed event surface delivered to Swift via the EventCallback trait.
//! Expanded in M4 to cover the full IrcEvent space (CTCP, whois, snotice
//! routing, away, etc.). M1 carries only the spike-era set.

#[derive(Debug, Clone, uniffi::Record)]
pub struct ChatMessage {
    pub server: String,
    pub nick: String,
    pub target: String,
    pub text: String,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum Event {
    Connected { server: String, nick: String },
    Disconnected { server: String, reason: String },
    Message { msg: ChatMessage },
    Join { server: String, nick: String, channel: String },
    Part { server: String, nick: String, channel: String },
    NickChange { server: String, old_nick: String, new_nick: String },
    Error { server: String, message: String },
}

#[uniffi::export(callback_interface)]
pub trait EventCallback: Send + Sync {
    fn on_event(&self, event: Event);
}
