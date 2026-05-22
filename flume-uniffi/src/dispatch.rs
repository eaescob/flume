//! IrcEvent → Event translation + buffer state mutation.
//!
//! One dispatcher per connected server. Called by the connection task on
//! every event from flume-core; produces zero or more FFI Events for the
//! Swift callback and mutates the per-server snapshot and buffer state.

use std::sync::{Arc, Mutex};

use flume_core::event::{ConnectionState as CoreConnectionState, IrcEvent, DisconnectReason};
use flume_core::irc::command::{Command, ParsedMessage};

use crate::buffers::{
    is_channel, parse_prefixed_nick, BufferKind, NickEntry, ServerBuffers,
};
use crate::events::Event;
use crate::servers::{ConnectionState, ServerSnapshot};

/// Compiled snotice rule. Mirrors the TUI's `CompiledSnoticeRule` but stays
/// in the FFI crate so flume-core doesn't grow a dep on it.
pub(crate) struct CompiledSnoticeRule {
    pub id: String,
    pub regex: regex::Regex,
    pub format: Option<String>,
    pub buffer: Option<String>,
    pub suppress: bool,
}

pub(crate) struct ServerDispatcher {
    pub server_name: String,
    pub snapshot: Arc<Mutex<ServerSnapshot>>,
    pub buffers: Arc<Mutex<ServerBuffers>>,
    pub snotice_rules: Arc<Mutex<Vec<CompiledSnoticeRule>>>,
}

impl ServerDispatcher {
    /// Process one IRC event, return zero or more FFI Events for the
    /// callback. Side effects: snapshot and buffer state mutations.
    pub fn handle(&self, ev: IrcEvent) -> Vec<Event> {
        let mut out = Vec::new();
        match ev {
            IrcEvent::Connected {
                server_name,
                our_nick,
                capabilities,
            } => {
                let caps: Vec<String> = capabilities.into_iter().collect();
                {
                    let mut s = self.snapshot.lock().expect("snapshot poisoned");
                    s.state = ConnectionState::Connected;
                    s.our_nick = Some(our_nick.clone());
                    s.capabilities = caps.clone();
                }
                // Server buffer always exists.
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .ensure("", BufferKind::Server);
                out.push(Event::Connected {
                    server: server_name,
                    nick: our_nick,
                    capabilities: caps,
                });
            }
            IrcEvent::Disconnected { server_name, reason } => {
                self.snapshot.lock().expect("snapshot poisoned").state =
                    ConnectionState::Disconnected;
                out.push(Event::Disconnected {
                    server: server_name,
                    reason: format_reason(&reason),
                });
            }
            IrcEvent::StateChanged { server_name, state } => {
                let ffi_state = core_state_to_ffi(&state);
                self.snapshot.lock().expect("snapshot poisoned").state = ffi_state.clone();
                out.push(Event::StateChanged {
                    server: server_name,
                    state: ffi_state,
                });
            }
            IrcEvent::Error { server_name, error } => {
                out.push(Event::Error {
                    server: server_name,
                    message: error,
                });
            }
            IrcEvent::MessageReceived { message, .. } => {
                self.dispatch_message(message, &mut out);
            }
        }
        out
    }

    fn our_nick(&self) -> Option<String> {
        self.snapshot.lock().expect("snapshot poisoned").our_nick.clone()
    }

    fn dispatch_message(&self, msg: ParsedMessage, out: &mut Vec<Event>) {
        let from_nick = msg
            .prefix
            .as_ref()
            .and_then(|p| p.nick())
            .unwrap_or_default()
            .to_string();
        let is_us = |nick: &str| {
            self.our_nick()
                .as_deref()
                .map(|us| us.eq_ignore_ascii_case(nick))
                .unwrap_or(false)
        };

        match msg.command {
            Command::Privmsg { target, text } => {
                let (is_action, text) = parse_action(&text);
                let from_is_us = is_us(&from_nick);
                let buffer_target = if is_channel(&target) {
                    target.clone()
                } else if !from_is_us {
                    // PM TO us → buffer keyed by sender's nick.
                    from_nick.clone()
                } else {
                    // Outgoing self-PM echo — bucket under recipient nick.
                    target.clone()
                };
                let kind = if is_channel(&buffer_target) {
                    BufferKind::Channel
                } else {
                    BufferKind::PrivateMessage
                };
                let mut bufs = self.buffers.lock().expect("buffers poisoned");
                bufs.ensure(&buffer_target, kind);
                // Own outgoing messages (echo-message capability) should not
                // count as unread or highlight — they're things we just sent.
                if !from_is_us {
                    let is_highlight = self
                        .our_nick()
                        .as_deref()
                        .map(|n| text.to_lowercase().contains(&n.to_lowercase()))
                        .unwrap_or(false);
                    bufs.increment_unread(&buffer_target, is_highlight);
                }
                drop(bufs);

                out.push(Event::Message {
                    server: self.server_name.clone(),
                    nick: from_nick,
                    target,
                    text,
                    is_action,
                });
            }
            Command::Notice { target, text } => {
                // Server notice → check snotice routing rules.
                let is_server_notice =
                    from_nick.is_empty() || from_nick.contains('.') || target == "*";
                if is_server_notice {
                    if self.try_route_snotice(&text, out) {
                        return;
                    }
                }
                out.push(Event::Notice {
                    server: self.server_name.clone(),
                    nick: from_nick,
                    target,
                    text,
                });
            }
            Command::Join { channels } => {
                for (chan, _key) in channels {
                    let mut bufs = self.buffers.lock().expect("buffers poisoned");
                    bufs.ensure(&chan, BufferKind::Channel);
                    bufs.add_nick(&chan, &from_nick, "");
                    drop(bufs);
                    out.push(Event::Join {
                        server: self.server_name.clone(),
                        nick: from_nick.clone(),
                        channel: chan,
                    });
                }
            }
            Command::Part { channels, message } => {
                for chan in channels {
                    let mut bufs = self.buffers.lock().expect("buffers poisoned");
                    bufs.remove_nick(&chan, &from_nick);
                    if is_us(&from_nick) {
                        bufs.remove(&chan);
                    }
                    drop(bufs);
                    out.push(Event::Part {
                        server: self.server_name.clone(),
                        nick: from_nick.clone(),
                        channel: chan,
                        reason: message.clone(),
                    });
                }
            }
            Command::Quit { message } => {
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .remove_nick_everywhere(&from_nick);
                out.push(Event::Quit {
                    server: self.server_name.clone(),
                    nick: from_nick,
                    reason: message,
                });
            }
            Command::Kick { channel, user, reason } => {
                let mut bufs = self.buffers.lock().expect("buffers poisoned");
                bufs.remove_nick(&channel, &user);
                if is_us(&user) {
                    bufs.remove(&channel);
                }
                drop(bufs);
                out.push(Event::Kick {
                    server: self.server_name.clone(),
                    kicker: from_nick,
                    target: user,
                    channel,
                    reason,
                });
            }
            Command::Nick { nickname } => {
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .rename_nick(&from_nick, &nickname);
                if is_us(&from_nick) {
                    self.snapshot.lock().expect("snapshot poisoned").our_nick =
                        Some(nickname.clone());
                }
                out.push(Event::NickChanged {
                    server: self.server_name.clone(),
                    old_nick: from_nick,
                    new_nick: nickname,
                });
            }
            Command::Topic { channel, topic } => {
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .set_topic(&channel, topic.clone());
                out.push(Event::TopicChanged {
                    server: self.server_name.clone(),
                    channel,
                    topic,
                    setter: if from_nick.is_empty() { None } else { Some(from_nick) },
                });
            }
            Command::Mode { target, modes, params } => {
                let modes_str = modes.clone().unwrap_or_default();
                if is_channel(&target) && !modes_str.is_empty() {
                    self.buffers
                        .lock()
                        .expect("buffers poisoned")
                        .apply_mode_to_nicks(&target, &modes_str, &params);
                }
                if !is_channel(&target) && is_us(&target) {
                    // User mode on us → update snapshot's user_modes.
                    if let Some(m) = &modes {
                        self.snapshot.lock().expect("snapshot poisoned").user_modes = m.clone();
                    }
                }
                out.push(Event::ModeChanged {
                    server: self.server_name.clone(),
                    target,
                    modes: modes_str,
                    params,
                    setter: if from_nick.is_empty() { None } else { Some(from_nick) },
                });
            }
            Command::Invite { nickname: _, channel } => {
                out.push(Event::Invited {
                    server: self.server_name.clone(),
                    channel,
                    from: from_nick,
                });
            }
            Command::Numeric { code, params } => {
                self.dispatch_numeric(code, params, out);
            }
            _ => {}
        }
    }

    fn dispatch_numeric(&self, code: u16, params: Vec<String>, out: &mut Vec<Event>) {
        match code {
            // RPL_TOPIC — params: [our_nick, channel, topic]
            332 if params.len() >= 3 => {
                let channel = params[1].clone();
                let topic = params[2].clone();
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .set_topic(&channel, Some(topic.clone()));
                out.push(Event::TopicChanged {
                    server: self.server_name.clone(),
                    channel,
                    topic: Some(topic),
                    setter: None,
                });
            }
            // RPL_NAMREPLY — params: [our_nick, "=|@|*", channel, "nick nick ..."]
            353 if params.len() >= 4 => {
                let channel = params[2].clone();
                let nicks: Vec<NickEntry> = params[3]
                    .split_whitespace()
                    .map(parse_prefixed_nick)
                    .collect();
                let mut bufs = self.buffers.lock().expect("buffers poisoned");
                bufs.ensure(&channel, BufferKind::Channel);
                // Replace, so the second 353 chunk doesn't accumulate dupes
                // until 366. Real cycles emit 353 in chunks then 366; for v1
                // simplicity we accept the small visible blip between chunks.
                bufs.set_nicks(&channel, nicks.clone());
                drop(bufs);
                out.push(Event::NamesUpdate {
                    server: self.server_name.clone(),
                    channel,
                    nicks,
                });
            }
            // RPL_CHANNELMODEIS — params: [our_nick, channel, modes...]
            324 if params.len() >= 3 => {
                let channel = params[1].clone();
                let modes = params[2..].join(" ");
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .set_channel_modes(&channel, Some(modes));
            }
            _ => {}
        }
    }

    fn try_route_snotice(&self, text: &str, out: &mut Vec<Event>) -> bool {
        let rules = self.snotice_rules.lock().expect("rules poisoned");
        for rule in rules.iter() {
            let Some(caps) = rule.regex.captures(text) else {
                continue;
            };
            if rule.suppress {
                out.push(Event::SnoticeRouted {
                    server: self.server_name.clone(),
                    rule_id: Some(rule.id.clone()),
                    buffer: rule.buffer.clone(),
                    original: text.to_string(),
                    formatted: String::new(),
                    suppressed: true,
                });
                return true;
            }
            let formatted = match &rule.format {
                Some(fmt) => flume_core::format::format_regex_captures(fmt, &caps),
                None => text.to_string(),
            };
            let buf_name = rule.buffer.clone().unwrap_or_default();
            if !buf_name.is_empty() {
                self.buffers
                    .lock()
                    .expect("buffers poisoned")
                    .ensure(&buf_name, BufferKind::Snotice);
                let mut bufs = self.buffers.lock().expect("buffers poisoned");
                bufs.increment_unread(&buf_name, false);
                drop(bufs);
            }
            out.push(Event::SnoticeRouted {
                server: self.server_name.clone(),
                rule_id: Some(rule.id.clone()),
                buffer: rule.buffer.clone(),
                original: text.to_string(),
                formatted,
                suppressed: false,
            });
            return true;
        }
        false
    }
}

fn parse_action(text: &str) -> (bool, String) {
    if text.starts_with('\x01') && text.ends_with('\x01') && text.len() >= 2 {
        let inner = &text[1..text.len() - 1];
        if let Some(rest) = inner.strip_prefix("ACTION ") {
            return (true, rest.to_string());
        }
    }
    (false, text.to_string())
}

fn core_state_to_ffi(s: &CoreConnectionState) -> ConnectionState {
    match s {
        CoreConnectionState::Disconnected => ConnectionState::Disconnected,
        CoreConnectionState::Connecting => ConnectionState::Connecting,
        CoreConnectionState::Registering => ConnectionState::Registering,
        CoreConnectionState::Connected => ConnectionState::Connected,
    }
}

fn format_reason(reason: &DisconnectReason) -> String {
    match reason {
        DisconnectReason::UserRequested => "user requested".to_string(),
        DisconnectReason::ServerClosed => "server closed".to_string(),
        DisconnectReason::PingTimeout => "ping timeout".to_string(),
        DisconnectReason::Error(e) => format!("error: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flume_core::event::{ConnectionState as CoreState, IrcEvent};
    use flume_core::irc::command::Command;
    use flume_core::irc::message::OwnedPrefix;

    fn dispatcher_for(server: &str, our_nick: Option<&str>) -> ServerDispatcher {
        ServerDispatcher {
            server_name: server.to_string(),
            snapshot: Arc::new(Mutex::new(ServerSnapshot {
                state: ConnectionState::Connected,
                our_nick: our_nick.map(String::from),
                user_modes: String::new(),
                capabilities: Vec::new(),
            })),
            buffers: Arc::new(Mutex::new(ServerBuffers::default())),
            snotice_rules: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn msg(prefix_nick: &str, command: Command) -> IrcEvent {
        IrcEvent::MessageReceived {
            server_name: "libera".into(),
            message: ParsedMessage {
                tags: Vec::new(),
                prefix: Some(OwnedPrefix::User {
                    nick: prefix_nick.into(),
                    user: None,
                    host: None,
                }),
                command,
                server_time: None,
            },
        }
    }

    #[test]
    fn privmsg_to_channel_creates_buffer_and_increments_unread() {
        let d = dispatcher_for("libera", Some("flmtest"));
        let events = d.handle(msg(
            "alice",
            Command::Privmsg {
                target: "#chan".into(),
                text: "hello".into(),
            },
        ));
        assert!(matches!(
            events[0],
            Event::Message {
                ref nick,
                ref target,
                ref text,
                is_action: false,
                ..
            } if nick == "alice" && target == "#chan" && text == "hello"
        ));
        let buffers = d.buffers.lock().unwrap();
        let buf = buffers.map.get("#chan").expect("buffer auto-created");
        assert!(matches!(buf.kind, BufferKind::Channel));
        assert_eq!(buf.unread, 1);
        assert_eq!(buf.highlights, 0);
    }

    #[test]
    fn privmsg_with_our_nick_highlights() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.handle(msg(
            "alice",
            Command::Privmsg {
                target: "#chan".into(),
                text: "hey FlmTest, ping".into(),
            },
        ));
        let buffers = d.buffers.lock().unwrap();
        assert_eq!(buffers.map.get("#chan").unwrap().highlights, 1);
    }

    #[test]
    fn private_message_buckets_under_sender() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.handle(msg(
            "alice",
            Command::Privmsg {
                target: "flmtest".into(),
                text: "hi".into(),
            },
        ));
        let buffers = d.buffers.lock().unwrap();
        let buf = buffers.map.get("alice").expect("PM buffer keyed by sender");
        assert!(matches!(buf.kind, BufferKind::PrivateMessage));
    }

    #[test]
    fn me_action_is_unwrapped() {
        let d = dispatcher_for("libera", Some("flmtest"));
        let events = d.handle(msg(
            "alice",
            Command::Privmsg {
                target: "#chan".into(),
                text: "\x01ACTION waves\x01".into(),
            },
        ));
        match &events[0] {
            Event::Message { text, is_action, .. } => {
                assert!(is_action);
                assert_eq!(text, "waves");
            }
            other => panic!("expected Event::Message, got {:?}", other),
        }
    }

    #[test]
    fn join_adds_nick_and_emits_event() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.handle(msg(
            "alice",
            Command::Join {
                channels: vec![("#chan".into(), None)],
            },
        ));
        let buffers = d.buffers.lock().unwrap();
        let buf = buffers.map.get("#chan").unwrap();
        assert_eq!(buf.nicks.len(), 1);
        assert_eq!(buf.nicks[0].nick, "alice");
    }

    #[test]
    fn part_by_self_removes_buffer() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.handle(msg(
            "flmtest",
            Command::Join {
                channels: vec![("#chan".into(), None)],
            },
        ));
        assert!(d.buffers.lock().unwrap().map.contains_key("#chan"));
        d.handle(msg(
            "flmtest",
            Command::Part {
                channels: vec!["#chan".into()],
                message: None,
            },
        ));
        assert!(!d.buffers.lock().unwrap().map.contains_key("#chan"));
    }

    #[test]
    fn nick_change_renames_in_all_channels() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.handle(msg(
            "alice",
            Command::Join {
                channels: vec![("#a".into(), None)],
            },
        ));
        d.handle(msg(
            "alice",
            Command::Join {
                channels: vec![("#b".into(), None)],
            },
        ));
        d.handle(msg(
            "alice",
            Command::Nick {
                nickname: "alice2".into(),
            },
        ));
        let buffers = d.buffers.lock().unwrap();
        assert_eq!(buffers.map.get("#a").unwrap().nicks[0].nick, "alice2");
        assert_eq!(buffers.map.get("#b").unwrap().nicks[0].nick, "alice2");
    }

    #[test]
    fn names_reply_populates_nick_list_with_prefixes() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.handle(IrcEvent::MessageReceived {
            server_name: "libera".into(),
            message: ParsedMessage {
                tags: Vec::new(),
                prefix: Some(OwnedPrefix::Server("libera.chat".into())),
                command: Command::Numeric {
                    code: 353,
                    params: vec![
                        "flmtest".into(),
                        "=".into(),
                        "#chan".into(),
                        "@alice +bob carol ~dave".into(),
                    ],
                },
                server_time: None,
            },
        });
        let buffers = d.buffers.lock().unwrap();
        let nicks = &buffers.map.get("#chan").unwrap().nicks;
        let order: Vec<&str> = nicks.iter().map(|n| n.nick.as_str()).collect();
        assert_eq!(order, vec!["dave", "alice", "bob", "carol"]);
    }

    #[test]
    fn topic_numeric_sets_buffer_topic() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.buffers
            .lock()
            .unwrap()
            .ensure("#chan", BufferKind::Channel);
        d.handle(IrcEvent::MessageReceived {
            server_name: "libera".into(),
            message: ParsedMessage {
                tags: Vec::new(),
                prefix: Some(OwnedPrefix::Server("libera.chat".into())),
                command: Command::Numeric {
                    code: 332,
                    params: vec!["flmtest".into(), "#chan".into(), "Welcome".into()],
                },
                server_time: None,
            },
        });
        assert_eq!(
            d.buffers.lock().unwrap().map.get("#chan").unwrap().topic,
            Some("Welcome".to_string())
        );
    }

    #[test]
    fn snotice_routing_creates_snotice_buffer() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.snotice_rules.lock().unwrap().push(CompiledSnoticeRule {
            id: "connect".into(),
            regex: regex::Regex::new(r"Client connecting: (\S+)").unwrap(),
            format: Some("[connect] ${1}".into()),
            buffer: Some("snotice-connections".into()),
            suppress: false,
        });
        d.handle(IrcEvent::MessageReceived {
            server_name: "libera".into(),
            message: ParsedMessage {
                tags: Vec::new(),
                prefix: Some(OwnedPrefix::Server("libera.chat".into())),
                command: Command::Notice {
                    target: "*".into(),
                    text: "Client connecting: alice".into(),
                },
                server_time: None,
            },
        });
        let buffers = d.buffers.lock().unwrap();
        let snotice = buffers
            .map
            .get("snotice-connections")
            .expect("snotice buffer auto-created");
        assert!(matches!(snotice.kind, BufferKind::Snotice));
        assert_eq!(snotice.unread, 1);
    }

    #[test]
    fn snotice_suppress_does_not_create_buffer() {
        let d = dispatcher_for("libera", Some("flmtest"));
        d.snotice_rules.lock().unwrap().push(CompiledSnoticeRule {
            id: "drop".into(),
            regex: regex::Regex::new(r"Oper-up").unwrap(),
            format: None,
            buffer: None,
            suppress: true,
        });
        let events = d.handle(IrcEvent::MessageReceived {
            server_name: "libera".into(),
            message: ParsedMessage {
                tags: Vec::new(),
                prefix: Some(OwnedPrefix::Server("libera.chat".into())),
                command: Command::Notice {
                    target: "*".into(),
                    text: "Oper-up alice".into(),
                },
                server_time: None,
            },
        });
        assert!(matches!(&events[0], Event::SnoticeRouted { suppressed: true, .. }));
        assert!(d.buffers.lock().unwrap().map.is_empty());
    }

    #[test]
    fn lifecycle_events_pass_through() {
        let d = dispatcher_for("libera", None);
        let events = d.handle(IrcEvent::Connected {
            server_name: "libera".into(),
            our_nick: "flmtest".into(),
            capabilities: ["sasl".into(), "server-time".into()].into_iter().collect(),
        });
        assert!(matches!(&events[0], Event::Connected { nick, .. } if nick == "flmtest"));
        assert!(d.buffers.lock().unwrap().map.contains_key(""));

        let events = d.handle(IrcEvent::StateChanged {
            server_name: "libera".into(),
            state: CoreState::Disconnected,
        });
        assert!(matches!(&events[0], Event::StateChanged { state: ConnectionState::Disconnected, .. }));
    }

    // Ensure the unused import warning stays away.
    #[allow(dead_code)]
    fn _touch_unused() {
        let _ = CoreState::Connecting;
    }
}
