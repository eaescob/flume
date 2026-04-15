use base64::Engine;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use crate::config::server::{AuthConfig, AuthMethod, SaslMechanism};
use crate::event::{ConnectionState, IrcEvent};
use crate::irc::command::{CapSubcommand, Command, ParsedMessage};
use crate::irc::message::OwnedIrcMessage;
use crate::irc::parser;

use super::transport::ConnectionError;

/// The set of IRCv3 capabilities we want to request.
const DESIRED_CAPS: &[&str] = &[
    "cap-notify",
    "sasl",
    "message-tags",
    "server-time",
    "echo-message",
    "away-notify",
    "extended-join",
    "account-tag",
    "multi-prefix",
    "batch",
    "labeled-response",
    "monitor",
    "sts",
    "znc.in/playback",
    "znc.in/self-message",
    "soju.im/bouncer-networks",
];

/// Result of a successful registration.
pub struct RegistrationResult {
    pub nick: String,
    pub capabilities: std::collections::HashSet<String>,
}

/// Perform IRC registration with CAP negotiation (if supported) and optional SASL.
///
/// This handles both modern IRCv3 servers and legacy RFC 1459 servers.
/// The approach: send CAP LS alongside NICK/USER. If we get a CAP response,
/// proceed with negotiation. If we get numeric replies first, the server
/// doesn't support CAP and we fall through to legacy registration.
pub async fn perform_registration<R: tokio::io::AsyncRead + Unpin>(
    write_tx: &mpsc::Sender<String>,
    reader: &mut BufReader<R>,
    nick: &str,
    username: &str,
    realname: &str,
    auth: &AuthConfig,
    server_password: Option<&str>,
    pass_username: Option<&str>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<IrcEvent>,
    server_name: &str,
) -> Result<RegistrationResult, ConnectionError> {
    let mut capabilities = std::collections::HashSet::new();
    let mut confirmed_nick = nick.to_string();

    // Send server password if configured.
    // When an explicit identity username is set alongside a server password,
    // combine them as "username:password" in the PASS command. This is
    // required for ZNC and other bouncers that authenticate via PASS.
    if let Some(pass) = server_password {
        if let Some(pu) = pass_username {
            send(write_tx, &format!("PASS {}:{}", pu, pass)).await?;
        } else {
            send(write_tx, &format!("PASS :{}", pass)).await?;
        }
    }

    // Send CAP LS + NICK + USER simultaneously
    send(write_tx, "CAP LS 302").await?;
    send(write_tx, &format!("NICK :{}", nick)).await?;
    send(write_tx, &format!("USER {} 0 * :{}", username, realname)).await?;

    let _ = event_tx.send(IrcEvent::StateChanged {
        server_name: server_name.to_string(),
        state: ConnectionState::Registering,
    });

    let mut sasl_in_progress = false;
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        let bytes_read = reader.read_line(&mut line_buf).await
            .map_err(ConnectionError::Io)?;

        if bytes_read == 0 {
            return Err(ConnectionError::ServerClosed);
        }

        let raw = line_buf.trim_end();
        if raw.is_empty() {
            continue;
        }

        tracing::debug!("<< {}", raw);

        let parsed = match parser::parse(raw) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!("Failed to parse server message: {} — {:?}", raw, e);
                continue;
            }
        };

        let owned = OwnedIrcMessage::from(parsed);
        let message = ParsedMessage::from_owned(owned);

        match &message.command {
            Command::Ping { token } => {
                send(write_tx, &format!("PONG :{}", token)).await?;
            }

            Command::Cap { subcommand } => match subcommand {
                CapSubcommand::Ls { caps, .. } => {
                    if let Some(caps_str) = caps {
                        let available: Vec<&str> = caps_str.split_whitespace().collect();

                        // Request capabilities that the server supports
                        let to_request: Vec<&str> = DESIRED_CAPS
                            .iter()
                            .filter(|cap| {
                                available.iter().any(|a| {
                                    // Server may advertise caps with values like "sasl=PLAIN,EXTERNAL"
                                    a.split('=').next().unwrap_or("") == **cap
                                })
                            })
                            .copied()
                            .collect();

                        if to_request.is_empty() {
                            send(write_tx, "CAP END").await?;
                        } else {
                            let req = to_request.join(" ");
                            send(write_tx, &format!("CAP REQ :{}", req)).await?;
                        }
                    }
                }

                CapSubcommand::Ack { caps } => {
                    for cap in caps.split_whitespace() {
                        capabilities.insert(cap.to_string());
                    }

                    // Start SASL if we got it and auth is configured for SASL
                    if capabilities.contains("sasl") && auth.method == AuthMethod::Sasl {
                        let mechanism = match auth.sasl_mechanism {
                            SaslMechanism::Plain => "PLAIN",
                            SaslMechanism::ScramSha256 => "SCRAM-SHA-256",
                            SaslMechanism::External => "EXTERNAL",
                        };
                        send(write_tx, &format!("AUTHENTICATE {}", mechanism)).await?;
                        sasl_in_progress = true;
                    } else {
                        send(write_tx, "CAP END").await?;
                    }
                }

                CapSubcommand::Nak { caps } => {
                    tracing::warn!("Server NAK'd capabilities: {}", caps);
                    if !sasl_in_progress {
                        send(write_tx, "CAP END").await?;
                    }
                }

                _ => {}
            },

            Command::Authenticate { data } => {
                if data == "+" && auth.method == AuthMethod::Sasl {
                    match auth.sasl_mechanism {
                        SaslMechanism::Plain => {
                            let sasl_data = encode_sasl_plain(
                                &auth.sasl_username,
                                &auth.sasl_username,
                                auth.sasl_password.as_deref().unwrap_or(""),
                            );
                            send(write_tx, &format!("AUTHENTICATE {}", sasl_data)).await?;
                        }
                        _ => {
                            // Only PLAIN is supported in Phase 1
                            tracing::warn!("Unsupported SASL mechanism, aborting SASL");
                            send(write_tx, "AUTHENTICATE *").await?;
                            sasl_in_progress = false;
                            send(write_tx, "CAP END").await?;
                        }
                    }
                }
            }

            Command::Numeric { code, params } => {
                match *code {
                    // RPL_LOGGEDIN (900)
                    900 => {
                        tracing::info!("SASL authentication successful");
                    }
                    // RPL_SASLSUCCESS (903)
                    903 => {
                        tracing::info!("SASL complete");
                        sasl_in_progress = false;
                        send(write_tx, "CAP END").await?;
                    }
                    // ERR_SASLFAIL (904), ERR_SASLTOOLONG (905), ERR_SASLABORTED (906)
                    904 | 905 | 906 => {
                        let reason = params.last().cloned().unwrap_or_default();
                        tracing::error!("SASL failed: {}", reason);
                        sasl_in_progress = false;
                        send(write_tx, "CAP END").await?;
                        // Don't abort — let the server decide if we can continue without auth
                    }
                    // RPL_WELCOME (001)
                    1 => {
                        if let Some(nick_param) = params.first() {
                            confirmed_nick = nick_param.clone();
                        }
                        break;
                    }
                    // ERR_NICKNAMEINUSE (433)
                    433 => {
                        // Try alt nick by appending underscore
                        confirmed_nick.push('_');
                        send(write_tx, &format!("NICK :{}", confirmed_nick)).await?;
                    }
                    _ => {}
                }
            }

            // Forward NOTICE and ERROR messages to the UI during registration
            Command::Notice { text, .. } => {
                let _ = event_tx.send(IrcEvent::MessageReceived {
                    server_name: server_name.to_string(),
                    message: message.clone(),
                });
                tracing::info!("[{}] Registration notice: {}", server_name, text);
            }

            Command::Raw { command, params } if command == "ERROR" => {
                let reason = params.first().cloned().unwrap_or_default();
                let _ = event_tx.send(IrcEvent::Error {
                    server_name: server_name.to_string(),
                    error: reason.clone(),
                });
                tracing::error!("[{}] Server ERROR during registration: {}", server_name, reason);
                return Err(ConnectionError::Registration(reason));
            }

            _ => {}
        }

    }

    Ok(RegistrationResult {
        nick: confirmed_nick,
        capabilities,
    })
}

/// Encode SASL PLAIN credentials: base64(authzid \0 authcid \0 password)
pub fn encode_sasl_plain(authzid: &str, authcid: &str, password: &str) -> String {
    let plain = format!("{}\0{}\0{}", authzid, authcid, password);
    base64::engine::general_purpose::STANDARD.encode(plain.as_bytes())
}

async fn send(tx: &mpsc::Sender<String>, line: &str) -> Result<(), ConnectionError> {
    tracing::debug!(">> {}", line);
    tx.send(line.to_string())
        .await
        .map_err(|_| ConnectionError::ServerClosed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sasl_plain_encoding() {
        let encoded = encode_sasl_plain("emilio", "emilio", "hunter2");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert_eq!(decoded_str, "emilio\0emilio\0hunter2");
    }

    #[test]
    fn sasl_plain_empty_password() {
        let encoded = encode_sasl_plain("user", "user", "");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert_eq!(decoded_str, "user\0user\0");
    }
}
