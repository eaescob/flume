//! Outgoing command surface. All methods push a UserCommand into the
//! per-server channel and return immediately.
//!
//! Where flume-core has a typed UserCommand variant we use it (so its
//! formatter handles trailing-param colons correctly). Everything else
//! ships as `UserCommand::RawLine` — the cost of duplicating the typed
//! enum across the FFI is higher than the value, and v1 doesn't have a
//! good reason to type every command.

use flume_core::event::UserCommand;

use crate::{buffers::BufferRef, error::FlumeError, FlumeClient};

impl FlumeClient {
    fn send_command(&self, server: &str, cmd: UserCommand) -> Result<(), FlumeError> {
        let state = self.state.lock().expect("state poisoned");
        let handle = state
            .servers
            .get(server)
            .ok_or_else(|| FlumeError::Network(format!("no connection to '{}'", server)))?;
        handle
            .command_tx
            .try_send(cmd)
            .map_err(|e| FlumeError::Connection(format!("command queue full: {}", e)))
    }

    fn send_raw_to(&self, server: &str, line: String) -> Result<(), FlumeError> {
        self.send_command(server, UserCommand::RawLine(line))
    }
}

fn require_target(buffer_ref: &BufferRef) -> Result<&str, FlumeError> {
    if buffer_ref.target.is_empty() {
        Err(FlumeError::InvalidArgument(
            "cannot send to the server buffer".into(),
        ))
    } else {
        Ok(&buffer_ref.target)
    }
}

#[uniffi::export]
impl FlumeClient {
    pub fn send_message(
        &self,
        buffer_ref: BufferRef,
        text: String,
    ) -> Result<(), FlumeError> {
        let target = require_target(&buffer_ref)?;
        self.send_command(
            &buffer_ref.server,
            UserCommand::SendMessage {
                target: target.to_string(),
                text,
            },
        )
    }

    pub fn send_action(
        &self,
        buffer_ref: BufferRef,
        text: String,
    ) -> Result<(), FlumeError> {
        let target = require_target(&buffer_ref)?;
        // CTCP ACTION: PRIVMSG target :\x01ACTION text\x01
        self.send_command(
            &buffer_ref.server,
            UserCommand::SendMessage {
                target: target.to_string(),
                text: format!("\x01ACTION {}\x01", text),
            },
        )
    }

    pub fn send_notice(
        &self,
        server: String,
        target: String,
        text: String,
    ) -> Result<(), FlumeError> {
        if target.is_empty() {
            return Err(FlumeError::InvalidArgument("target is empty".into()));
        }
        self.send_raw_to(&server, format!("NOTICE {} :{}", target, text))
    }

    pub fn join_channel(
        &self,
        server: String,
        channel: String,
        key: Option<String>,
    ) -> Result<(), FlumeError> {
        if channel.is_empty() {
            return Err(FlumeError::InvalidArgument("channel is empty".into()));
        }
        self.send_command(&server, UserCommand::Join { channel, key })
    }

    pub fn part_channel(
        &self,
        server: String,
        channel: String,
        message: Option<String>,
    ) -> Result<(), FlumeError> {
        if channel.is_empty() {
            return Err(FlumeError::InvalidArgument("channel is empty".into()));
        }
        self.send_command(&server, UserCommand::Part { channel, message })
    }

    pub fn change_nick(&self, server: String, nick: String) -> Result<(), FlumeError> {
        if nick.is_empty() {
            return Err(FlumeError::InvalidArgument("nick is empty".into()));
        }
        self.send_command(&server, UserCommand::ChangeNick(nick))
    }

    pub fn set_topic(
        &self,
        server: String,
        channel: String,
        topic: Option<String>,
    ) -> Result<(), FlumeError> {
        if channel.is_empty() {
            return Err(FlumeError::InvalidArgument("channel is empty".into()));
        }
        let line = match topic {
            Some(t) => format!("TOPIC {} :{}", channel, t),
            None => format!("TOPIC {}", channel),
        };
        self.send_raw_to(&server, line)
    }

    pub fn kick(
        &self,
        server: String,
        channel: String,
        nick: String,
        reason: Option<String>,
    ) -> Result<(), FlumeError> {
        if channel.is_empty() || nick.is_empty() {
            return Err(FlumeError::InvalidArgument(
                "channel and nick are required".into(),
            ));
        }
        let line = match reason {
            Some(r) => format!("KICK {} {} :{}", channel, nick, r),
            None => format!("KICK {} {}", channel, nick),
        };
        self.send_raw_to(&server, line)
    }

    pub fn set_mode(
        &self,
        server: String,
        target: String,
        modes: String,
        params: Vec<String>,
    ) -> Result<(), FlumeError> {
        if target.is_empty() || modes.is_empty() {
            return Err(FlumeError::InvalidArgument(
                "target and modes are required".into(),
            ));
        }
        let line = if params.is_empty() {
            format!("MODE {} {}", target, modes)
        } else {
            format!("MODE {} {} {}", target, modes, params.join(" "))
        };
        self.send_raw_to(&server, line)
    }

    pub fn whois(&self, server: String, nick: String) -> Result<(), FlumeError> {
        if nick.is_empty() {
            return Err(FlumeError::InvalidArgument("nick is empty".into()));
        }
        self.send_raw_to(&server, format!("WHOIS {}", nick))
    }

    pub fn invite(
        &self,
        server: String,
        channel: String,
        nick: String,
    ) -> Result<(), FlumeError> {
        if channel.is_empty() || nick.is_empty() {
            return Err(FlumeError::InvalidArgument(
                "channel and nick are required".into(),
            ));
        }
        self.send_raw_to(&server, format!("INVITE {} {}", nick, channel))
    }

    pub fn send_raw(&self, server: String, line: String) -> Result<(), FlumeError> {
        if line.is_empty() {
            return Err(FlumeError::InvalidArgument("line is empty".into()));
        }
        self.send_raw_to(&server, line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FlumeClient;

    fn fresh_client() -> std::sync::Arc<FlumeClient> {
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("FLUME_MAC_CONFIG_DIR", tmp.path());
        let c = FlumeClient::new();
        // Leak the tmp dir for the test process lifetime — keeps the path
        // valid for any followup reads inside the FlumeClient.
        std::mem::forget(tmp);
        c
    }

    #[test]
    fn unknown_server_returns_network_error() {
        let client = fresh_client();
        let err = client
            .send_message(
                BufferRef {
                    server: "ghost".into(),
                    target: "#chan".into(),
                },
                "hi".into(),
            )
            .unwrap_err();
        assert!(matches!(err, FlumeError::Network(_)));
    }

    #[test]
    fn empty_target_rejected() {
        let client = fresh_client();
        let err = client
            .send_message(
                BufferRef {
                    server: "ghost".into(),
                    target: "".into(),
                },
                "hi".into(),
            )
            .unwrap_err();
        assert!(matches!(err, FlumeError::InvalidArgument(_)));
    }

    #[test]
    fn empty_nick_change_rejected() {
        let client = fresh_client();
        let err = client.change_nick("ghost".into(), "".into()).unwrap_err();
        assert!(matches!(err, FlumeError::InvalidArgument(_)));
    }

    /// Live IRC test — connects to Libera, joins a unique channel, sends
    /// a PRIVMSG, and verifies the echo-message capability surfaces it
    /// back as an Event::Message. Proves the full write → wire → echo →
    /// dispatcher → callback chain.
    ///
    /// Run with: cargo test -p flume-uniffi -- --ignored live_send_message
    #[test]
    #[ignore]
    #[serial_test::serial]
    fn live_send_message_echoes_back() {
        use crate::events::{Event, EventCallback};
        use crate::servers::{AuthConfig, AuthMethod, BouncerType, NetworkConfig};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        struct Collect(Arc<Mutex<Vec<Event>>>);
        impl EventCallback for Collect {
            fn on_event(&self, event: Event) {
                self.0.lock().unwrap().push(event);
            }
        }

        let client = fresh_client();
        let events = Arc::new(Mutex::new(Vec::new()));
        let cb: Box<dyn EventCallback> = Box::new(Collect(Arc::clone(&events)));
        client.set_event_callback(cb);

        let pid = std::process::id();
        let nick = format!("flmtest{}", pid % 1000);
        let chan = format!("#flumemac-test-{}", pid);

        let cfg = NetworkConfig {
            name: "libera".into(),
            address: "irc.libera.chat".into(),
            port: 6697,
            tls: true,
            tls_accept_invalid_certs: false,
            nick: Some(nick.clone()),
            username: Some(nick.clone()),
            realname: Some("FlumeMac spike".into()),
            autojoin: vec![],
            auth: AuthConfig {
                method: AuthMethod::None,
                username: None,
                password: None,
            },
            autoconnect: false,
            bouncer: BouncerType::None,
            password: None,
        };
        client.add_network(cfg).unwrap();
        client.connect_network("libera".into()).unwrap();

        // Wait for Connected
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let connected = events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, Event::Connected { .. }));
            if connected {
                break;
            }
            assert!(Instant::now() < deadline, "no Connected after 15s");
            std::thread::sleep(Duration::from_millis(200));
        }

        // Join a unique channel and wait for the echo
        client
            .join_channel("libera".into(), chan.clone(), None)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let has_join = events.lock().unwrap().iter().any(
                |e| matches!(e, Event::Join { nick: n, channel: c, .. } if n.eq_ignore_ascii_case(&nick) && c.eq_ignore_ascii_case(&chan)),
            );
            if has_join {
                break;
            }
            if Instant::now() >= deadline {
                let snapshot: Vec<_> = events.lock().unwrap().clone();
                panic!(
                    "no Join echo for {} in {} after 15s. {} events observed: {:#?}",
                    nick,
                    chan,
                    snapshot.len(),
                    snapshot,
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        // Send a unique-payload message
        let payload = format!("flumemac-spike-{}", pid);
        client
            .send_message(
                BufferRef {
                    server: "libera".into(),
                    target: chan.clone(),
                },
                payload.clone(),
            )
            .unwrap();

        // Wait for the echo-message back via our callback
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let has_echo = events.lock().unwrap().iter().any(|e| {
                matches!(e, Event::Message { nick: n, target: t, text, .. }
                    if n.eq_ignore_ascii_case(&nick)
                        && t.eq_ignore_ascii_case(&chan)
                        && text == &payload)
            });
            if has_echo {
                break;
            }
            assert!(Instant::now() < deadline, "no Message echo after 10s");
            std::thread::sleep(Duration::from_millis(100));
        }

        client.disconnect_network("libera".into(), None);
    }
}
