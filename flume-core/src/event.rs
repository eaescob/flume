use crate::irc::command::ParsedMessage;

/// Events emitted by a ServerConnection, consumed by the TUI and other subsystems.
#[derive(Debug, Clone)]
pub enum IrcEvent {
    /// Successfully connected and registered with the server.
    Connected {
        server_name: String,
        our_nick: String,
    },
    /// Disconnected from the server.
    Disconnected {
        server_name: String,
        reason: DisconnectReason,
    },
    /// A parsed IRC message received from the server.
    MessageReceived {
        server_name: String,
        message: ParsedMessage,
    },
    /// Connection state changed.
    StateChanged {
        server_name: String,
        state: ConnectionState,
    },
    /// An error occurred.
    Error {
        server_name: String,
        error: String,
    },
}

/// Why a connection was closed.
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    UserRequested,
    ServerClosed,
    Error(String),
    PingTimeout,
}

/// Connection lifecycle states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Registering,
    Connected,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Disconnected => write!(f, "disconnected"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Registering => write!(f, "registering"),
            ConnectionState::Connected => write!(f, "connected"),
        }
    }
}

/// Commands sent from the TUI/user to a ServerConnection.
#[derive(Debug, Clone)]
pub enum UserCommand {
    /// Send a PRIVMSG to a target.
    SendMessage { target: String, text: String },
    /// Join a channel.
    Join { channel: String, key: Option<String> },
    /// Part a channel.
    Part { channel: String, message: Option<String> },
    /// Change nick.
    ChangeNick(String),
    /// Quit with optional message.
    Quit(Option<String>),
    /// Send a raw IRC line.
    RawLine(String),
}
