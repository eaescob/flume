pub mod registration;
pub mod sts;
pub mod transport;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::config::general::{CtcpConfig, GeneralConfig};
use crate::config::server::ServerConfig;
use crate::config::vault::{resolve_secrets, Vault};
use crate::event::{ConnectionState, DisconnectReason, IrcEvent, UserCommand};
use crate::irc::command::{Command, ParsedMessage};
use crate::irc::message::OwnedIrcMessage;
use crate::irc::parser;

use transport::ConnectionError;

const COMMAND_CHANNEL_CAPACITY: usize = 256;
const WRITE_CHANNEL_CAPACITY: usize = 256;

/// Handles for interacting with a ServerConnection from outside.
pub struct ConnectionHandle {
    pub event_rx: mpsc::UnboundedReceiver<IrcEvent>,
    pub command_tx: mpsc::Sender<UserCommand>,
}

/// A single IRC server connection managed as an async task.
pub struct ServerConnection {
    server_config: ServerConfig,
    general_config: GeneralConfig,
    ctcp_config: CtcpConfig,
    vault: Option<Vault>,
    event_tx: mpsc::UnboundedSender<IrcEvent>,
    command_rx: mpsc::Receiver<UserCommand>,
}

impl ServerConnection {
    /// Create a new ServerConnection. Returns the connection and a handle
    /// with channels for the caller to interact with it.
    pub fn new(
        server_config: ServerConfig,
        general_config: GeneralConfig,
        vault: Option<Vault>,
        ctcp_config: CtcpConfig,
    ) -> (Self, ConnectionHandle) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);

        let conn = ServerConnection {
            server_config,
            general_config,
            ctcp_config,
            vault,
            event_tx,
            command_rx,
        };

        let handle = ConnectionHandle {
            event_rx,
            command_tx,
        };

        (conn, handle)
    }

    /// Run the connection. This is the main entry point — spawn this as a tokio task.
    pub async fn run(mut self) {
        let server_name = self.server_config.server.name.clone();
        let max_attempts = self.server_config.advanced.reconnect_attempts;
        let reconnect_delay = Duration::from_millis(self.server_config.advanced.reconnect_delay_ms);

        for attempt in 0..=max_attempts {
            if attempt > 0 {
                tracing::info!(
                    "[{}] Reconnect attempt {}/{}",
                    server_name,
                    attempt,
                    max_attempts
                );
                tokio::time::sleep(reconnect_delay).await;
            }

            let _ = self.event_tx.send(IrcEvent::StateChanged {
                server_name: server_name.clone(),
                state: ConnectionState::Connecting,
            });

            match self.connect_and_run().await {
                Ok(()) => {
                    // Clean disconnect (user requested quit)
                    let _ = self.event_tx.send(IrcEvent::Disconnected {
                        server_name: server_name.clone(),
                        reason: DisconnectReason::UserRequested,
                    });
                    return;
                }
                Err(e) => {
                    tracing::error!("[{}] Connection error: {}", server_name, e);
                    let _ = self.event_tx.send(IrcEvent::Disconnected {
                        server_name: server_name.clone(),
                        reason: DisconnectReason::Error(e.to_string()),
                    });
                }
            }
        }

        tracing::error!(
            "[{}] Max reconnect attempts ({}) exhausted",
            server_name,
            max_attempts
        );
    }

    async fn connect_and_run(&mut self) -> Result<(), ConnectionError> {
        let server_name = self.server_config.server.name.clone();
        let address = &self.server_config.server.address;
        let port = self.server_config.server.port;

        // Establish connection (TLS or plain)
        tracing::info!("[{}] Connecting to {}:{}", server_name, address, port);
        let stream = if self.server_config.server.tls {
            let accept_invalid = self.server_config.server.tls_accept_invalid_certs;
            transport::connect_tls_with_options(address, port, accept_invalid).await?
        } else {
            transport::connect_plain(address, port).await?
        };

        let (read_half, write_half) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);

        // Spawn write loop
        let (write_tx, mut write_rx) = mpsc::channel::<String>(WRITE_CHANNEL_CAPACITY);
        let flood_delay = Duration::from_millis(self.server_config.advanced.flood_delay_ms);
        let mut write_half = write_half;
        tokio::spawn(async move {
            while let Some(line) = write_rx.recv().await {
                let data = format!("{}\r\n", line);
                if let Err(e) = write_half.write_all(data.as_bytes()).await {
                    tracing::error!("Write error: {}", e);
                    break;
                }
                if let Err(e) = write_half.flush().await {
                    tracing::error!("Flush error: {}", e);
                    break;
                }
                if !flood_delay.is_zero() {
                    tokio::time::sleep(flood_delay).await;
                }
            }
        });

        // Resolve secrets for auth
        let nick = self
            .server_config
            .identity
            .nick
            .as_deref()
            .unwrap_or(&self.general_config.default_nick);
        let username = self
            .server_config
            .identity
            .username
            .as_deref()
            .unwrap_or(&self.general_config.username);
        let realname = self
            .server_config
            .identity
            .realname
            .as_deref()
            .unwrap_or(&self.general_config.realname);

        // Resolve secrets in auth config
        let mut auth = self.server_config.auth.clone();
        if let Some(ref pw) = auth.sasl_password {
            auth.sasl_password = Some(resolve_secrets(pw, self.vault.as_ref()));
        }
        if let Some(ref pw) = auth.nickserv_password {
            auth.nickserv_password = Some(resolve_secrets(pw, self.vault.as_ref()));
        }

        let server_password = self
            .server_config
            .server
            .password
            .as_deref()
            .map(|p| resolve_secrets(p, self.vault.as_ref()));

        // If an explicit identity username is set, pass it for PASS command
        // so bouncers (ZNC etc.) get "username:password" in PASS.
        let pass_username = self
            .server_config
            .identity
            .username
            .as_deref();

        // Registration
        let result = registration::perform_registration(
            &write_tx,
            &mut reader,
            nick,
            username,
            realname,
            &auth,
            server_password.as_deref(),
            pass_username,
            &self.event_tx,
            &server_name,
        )
        .await?;

        tracing::info!(
            "[{}] Registered as {} with caps: {:?}",
            server_name,
            result.nick,
            result.capabilities
        );

        let _ = self.event_tx.send(IrcEvent::Connected {
            server_name: server_name.clone(),
            our_nick: result.nick.clone(),
            capabilities: result.capabilities.clone(),
        });

        let _ = self.event_tx.send(IrcEvent::StateChanged {
            server_name: server_name.clone(),
            state: ConnectionState::Connected,
        });

        // Main read/write loop
        self.main_loop(&write_tx, &mut reader, &server_name, &result.nick)
            .await
    }

    async fn main_loop<R: tokio::io::AsyncRead + Unpin>(
        &mut self,
        write_tx: &mpsc::Sender<String>,
        reader: &mut BufReader<R>,
        server_name: &str,
        initial_nick: &str,
    ) -> Result<(), ConnectionError> {
        let mut our_nick = initial_nick.to_string();
        let mut line_buf = String::new();
        let mut last_activity = Instant::now();
        let ping_interval = Duration::from_secs(60);
        let ping_timeout = Duration::from_secs(30);
        let mut awaiting_pong = false;
        let mut pong_deadline: Option<Instant> = None;
        // CTCP rate limiting: track last response time per source nick
        let mut ctcp_last_response: HashMap<String, Instant> = HashMap::new();
        // BATCH buffering: active batches hold messages until completion
        let mut active_batches: HashMap<String, Vec<ParsedMessage>> = HashMap::new();

        loop {
            let timeout_duration = if awaiting_pong {
                pong_deadline
                    .map(|d| d.saturating_duration_since(Instant::now()))
                    .unwrap_or(ping_timeout)
            } else {
                ping_interval
                    .checked_sub(last_activity.elapsed())
                    .unwrap_or(Duration::ZERO)
            };

            tokio::select! {
                // Read from server
                result = async {
                    line_buf.clear();
                    reader.read_line(&mut line_buf).await
                } => {
                    let bytes_read = result.map_err(ConnectionError::Io)?;
                    if bytes_read == 0 {
                        return Err(ConnectionError::ServerClosed);
                    }

                    last_activity = Instant::now();
                    awaiting_pong = false;
                    pong_deadline = None;

                    let raw = line_buf.trim_end();
                    if raw.is_empty() {
                        continue;
                    }

                    tracing::trace!("<< {}", raw);

                    let parsed = match parser::parse(raw) {
                        Ok(msg) => msg,
                        Err(e) => {
                            tracing::warn!("Parse error: {} — {:?}", raw, e);
                            continue;
                        }
                    };

                    let owned = OwnedIrcMessage::from(parsed);
                    let message = ParsedMessage::from_owned(owned);

                    // Handle PING/PONG internally
                    if let Command::Ping { ref token } = message.command {
                        let pong = format!("PONG :{}", token);
                        let _ = write_tx.send(pong).await;
                        continue;
                    }
                    if matches!(message.command, Command::Pong { .. }) {
                        continue;
                    }

                    // Track nick changes
                    if let Command::Nick { ref nickname } = message.command {
                        if message.prefix_nick() == Some(&our_nick) {
                            our_nick = nickname.clone();
                        }
                    }

                    // CTCP auto-response for PRIVMSG containing \x01...\x01
                    if let Command::Privmsg { ref text, .. } = message.command {
                        if let Some(ctcp_cmd) = extract_ctcp(text) {
                            if ctcp_cmd.command != "ACTION" {
                                if let Some(sender) = message.prefix_nick() {
                                    let rate_limit = Duration::from_secs(self.ctcp_config.rate_limit as u64);
                                    let now = Instant::now();
                                    let rate_ok = ctcp_last_response
                                        .get(sender)
                                        .map(|t| now.duration_since(*t) >= rate_limit)
                                        .unwrap_or(true);

                                    if rate_ok {
                                        let response = match ctcp_cmd.command {
                                            "VERSION" if self.ctcp_config.respond_to_version => {
                                                Some(format!("VERSION {}", self.ctcp_config.version_reply))
                                            }
                                            "PING" if self.ctcp_config.respond_to_ping => {
                                                Some(format!("PING {}", ctcp_cmd.params))
                                            }
                                            "TIME" if self.ctcp_config.respond_to_time => {
                                                let time = chrono::Local::now().format("%a %b %d %H:%M:%S %Y").to_string();
                                                Some(format!("TIME {}", time))
                                            }
                                            "CLIENTINFO" => {
                                                Some("CLIENTINFO ACTION PING VERSION TIME CLIENTINFO".to_string())
                                            }
                                            _ => None,
                                        };

                                        if let Some(reply) = response {
                                            let line = format!("NOTICE {} :\x01{}\x01", sender, reply);
                                            let _ = write_tx.send(line).await;
                                            ctcp_last_response.insert(sender.to_string(), now);
                                            tracing::debug!("[{}] CTCP {} reply to {}", server_name, ctcp_cmd.command, sender);
                                        }
                                    } else {
                                        tracing::debug!("[{}] CTCP {} from {} rate-limited", server_name, ctcp_cmd.command, sender);
                                    }
                                }
                            }
                        }
                    }

                    // Handle BATCH protocol
                    if let Command::Batch { ref reference, .. } = message.command {
                        if reference.starts_with('+') {
                            let ref_id = reference[1..].to_string();
                            tracing::trace!("[{}] BATCH start: {}", server_name, ref_id);
                            active_batches.insert(ref_id, Vec::new());
                        } else if reference.starts_with('-') {
                            let ref_id = reference[1..].to_string();
                            if let Some(messages) = active_batches.remove(&ref_id) {
                                tracing::trace!("[{}] BATCH end: {} ({} messages)", server_name, ref_id, messages.len());
                                for msg in messages {
                                    let _ = self.event_tx.send(IrcEvent::MessageReceived {
                                        server_name: server_name.to_string(),
                                        message: msg,
                                    });
                                }
                            }
                        }
                        continue;
                    }

                    // Check if message belongs to an active batch
                    let batch_ref: Option<String> = message.tags.iter()
                        .find(|t| t.key == "batch")
                        .and_then(|t| t.value.clone());
                    if let Some(ref batch_id) = batch_ref {
                        if let Some(batch) = active_batches.get_mut(batch_id.as_str()) {
                            batch.push(message);
                            continue;
                        }
                    }

                    // Broadcast to consumers
                    let _ = self.event_tx.send(IrcEvent::MessageReceived {
                        server_name: server_name.to_string(),
                        message,
                    });
                }

                // Receive user commands
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(UserCommand::Quit(msg)) => {
                            let quit_msg = msg.unwrap_or_else(|| self.general_config.quit_message.clone());
                            let _ = write_tx.send(format!("QUIT :{}", quit_msg)).await;
                            // Give server a moment to process
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            return Ok(());
                        }
                        Some(UserCommand::SendMessage { target, text }) => {
                            let _ = write_tx.send(format!("PRIVMSG {} :{}", target, text)).await;
                        }
                        Some(UserCommand::Join { channel, key }) => {
                            let line = match key {
                                Some(k) => format!("JOIN {} :{}", channel, k),
                                None => format!("JOIN :{}", channel),
                            };
                            let _ = write_tx.send(line).await;
                        }
                        Some(UserCommand::Part { channel, message }) => {
                            let line = match message {
                                Some(m) => format!("PART {} :{}", channel, m),
                                None => format!("PART :{}", channel),
                            };
                            let _ = write_tx.send(line).await;
                        }
                        Some(UserCommand::ChangeNick(nick)) => {
                            let _ = write_tx.send(format!("NICK :{}", nick)).await;
                        }
                        Some(UserCommand::RawLine(line)) => {
                            let _ = write_tx.send(line).await;
                        }
                        None => {
                            // Command channel closed, shut down
                            return Ok(());
                        }
                    }
                }

                // Ping/pong keepalive timer
                _ = tokio::time::sleep(timeout_duration) => {
                    if awaiting_pong {
                        return Err(ConnectionError::PingTimeout);
                    } else {
                        let token = format!("flume-{}", last_activity.elapsed().as_secs());
                        let _ = write_tx.send(format!("PING :{}", token)).await;
                        awaiting_pong = true;
                        pong_deadline = Some(Instant::now() + ping_timeout);
                    }
                }
            }
        }
    }
}

/// Extracted CTCP command from a PRIVMSG.
struct CtcpCommand<'a> {
    command: &'a str,
    params: &'a str,
}

/// Extract a CTCP command from a PRIVMSG text if it's wrapped in \x01.
fn extract_ctcp(text: &str) -> Option<CtcpCommand<'_>> {
    let text = text.strip_prefix('\x01')?.strip_suffix('\x01')?;
    if text.is_empty() {
        return None;
    }
    let (command, params) = match text.find(' ') {
        Some(i) => (&text[..i], text[i + 1..].trim()),
        None => (text, ""),
    };
    Some(CtcpCommand { command, params })
}
