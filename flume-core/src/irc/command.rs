use crate::irc::message::{OwnedIrcMessage, OwnedPrefix, OwnedTag};

/// Strongly-typed IRC command parsed from an IrcMessage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // Connection registration
    Pass { password: String },
    Nick { nickname: String },
    User { username: String, realname: String },
    Quit { message: Option<String> },

    // Channel operations
    Join { channels: Vec<(String, Option<String>)> },
    Part { channels: Vec<String>, message: Option<String> },
    Topic { channel: String, topic: Option<String> },
    Names { channels: Vec<String> },
    Kick { channel: String, user: String, reason: Option<String> },
    Invite { nickname: String, channel: String },

    // Messaging
    Privmsg { target: String, text: String },
    Notice { target: String, text: String },

    // Server
    Ping { token: String },
    Pong { token: String },
    Mode { target: String, modes: Option<String>, params: Vec<String> },

    // IRCv3
    Cap { subcommand: CapSubcommand },
    Authenticate { data: String },

    // Numeric replies
    Numeric { code: u16, params: Vec<String> },

    // Catch-all
    Raw { command: String, params: Vec<String> },
}

/// IRCv3 CAP subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapSubcommand {
    Ls { version: Option<String>, caps: Option<String> },
    List { caps: Option<String> },
    Req { caps: String },
    Ack { caps: String },
    Nak { caps: String },
    New { caps: String },
    Del { caps: String },
    End,
}

/// A fully-parsed IRC message combining prefix, tags, and typed command.
#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub tags: Vec<OwnedTag>,
    pub prefix: Option<OwnedPrefix>,
    pub command: Command,
    pub server_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl Command {
    /// Parse a typed Command from an OwnedIrcMessage.
    pub fn from_message(msg: &OwnedIrcMessage) -> Command {
        let cmd = msg.command.to_uppercase();
        match cmd.as_str() {
            "PASS" => Command::Pass {
                password: msg.params.first().cloned().unwrap_or_default(),
            },
            "NICK" => Command::Nick {
                nickname: msg.params.first().cloned().unwrap_or_default(),
            },
            "USER" => Command::User {
                username: msg.params.first().cloned().unwrap_or_default(),
                realname: msg.params.last().cloned().unwrap_or_default(),
            },
            "QUIT" => Command::Quit {
                message: msg.params.first().cloned(),
            },
            "JOIN" => {
                let channel_str = msg.params.first().cloned().unwrap_or_default();
                let key_str = msg.params.get(1).cloned();
                let channels: Vec<&str> = channel_str.split(',').collect();
                let keys: Vec<Option<&str>> = match key_str {
                    Some(ref k) => k.split(',').map(Some).collect(),
                    None => Vec::new(),
                };
                let pairs = channels
                    .into_iter()
                    .enumerate()
                    .map(|(i, ch)| {
                        let key = keys.get(i).copied().flatten().map(String::from);
                        (ch.to_string(), key)
                    })
                    .collect();
                Command::Join { channels: pairs }
            }
            "PART" => {
                let channel_str = msg.params.first().cloned().unwrap_or_default();
                let channels = channel_str.split(',').map(String::from).collect();
                let message = msg.params.get(1).cloned();
                Command::Part { channels, message }
            }
            "TOPIC" => Command::Topic {
                channel: msg.params.first().cloned().unwrap_or_default(),
                topic: msg.params.get(1).cloned(),
            },
            "NAMES" => {
                let channels = msg
                    .params
                    .first()
                    .map(|s| s.split(',').map(String::from).collect())
                    .unwrap_or_default();
                Command::Names { channels }
            }
            "KICK" => Command::Kick {
                channel: msg.params.first().cloned().unwrap_or_default(),
                user: msg.params.get(1).cloned().unwrap_or_default(),
                reason: msg.params.get(2).cloned(),
            },
            "INVITE" => Command::Invite {
                nickname: msg.params.first().cloned().unwrap_or_default(),
                channel: msg.params.get(1).cloned().unwrap_or_default(),
            },
            "PRIVMSG" => Command::Privmsg {
                target: msg.params.first().cloned().unwrap_or_default(),
                text: msg.params.get(1).cloned().unwrap_or_default(),
            },
            "NOTICE" => Command::Notice {
                target: msg.params.first().cloned().unwrap_or_default(),
                text: msg.params.get(1).cloned().unwrap_or_default(),
            },
            "PING" => Command::Ping {
                token: msg.params.first().cloned().unwrap_or_default(),
            },
            "PONG" => Command::Pong {
                token: msg.params.first().cloned().unwrap_or_default(),
            },
            "MODE" => {
                let target = msg.params.first().cloned().unwrap_or_default();
                let modes = msg.params.get(1).cloned();
                let params = msg.params.get(2..).unwrap_or_default().to_vec();
                Command::Mode { target, modes, params }
            }
            "CAP" => {
                // CAP params: [*|nick] subcommand [:data]
                // The first param after CAP may be `*` (during registration) or our nick
                let sub_idx = if msg.params.len() >= 2 { 1 } else { 0 };
                let subcmd = msg.params.get(sub_idx).map(|s| s.as_str()).unwrap_or("");
                let data = msg.params.get(sub_idx + 1).cloned();

                let subcommand = match subcmd.to_uppercase().as_str() {
                    "LS" => {
                        // CAP * LS :caps  OR  CAP * LS 302
                        // If data is a number, it's the version and caps come next
                        // If data contains spaces or letters, it's the caps
                        let (version, caps) = match data {
                            Some(ref d) if d.parse::<u32>().is_ok() => {
                                (Some(d.clone()), msg.params.get(sub_idx + 2).cloned())
                            }
                            other => (None, other),
                        };
                        CapSubcommand::Ls { version, caps }
                    }
                    "LIST" => CapSubcommand::List { caps: data },
                    "REQ" => CapSubcommand::Req {
                        caps: data.unwrap_or_default(),
                    },
                    "ACK" => CapSubcommand::Ack {
                        caps: data.unwrap_or_default(),
                    },
                    "NAK" => CapSubcommand::Nak {
                        caps: data.unwrap_or_default(),
                    },
                    "NEW" => CapSubcommand::New {
                        caps: data.unwrap_or_default(),
                    },
                    "DEL" => CapSubcommand::Del {
                        caps: data.unwrap_or_default(),
                    },
                    "END" => CapSubcommand::End,
                    _ => {
                        return Command::Raw {
                            command: "CAP".to_string(),
                            params: msg.params.clone(),
                        };
                    }
                };
                Command::Cap { subcommand }
            }
            "AUTHENTICATE" => Command::Authenticate {
                data: msg.params.first().cloned().unwrap_or_default(),
            },
            _ => {
                // Try to parse as numeric
                if let Ok(code) = cmd.parse::<u16>() {
                    Command::Numeric {
                        code,
                        params: msg.params.clone(),
                    }
                } else {
                    Command::Raw {
                        command: msg.command.clone(),
                        params: msg.params.clone(),
                    }
                }
            }
        }
    }

    /// Convert to a raw IRC line (without \r\n).
    pub fn to_raw(&self) -> String {
        match self {
            Command::Pass { password } => format!("PASS :{}", password),
            Command::Nick { nickname } => format!("NICK :{}", nickname),
            Command::User { username, realname } => {
                format!("USER {} 0 * :{}", username, realname)
            }
            Command::Quit { message } => match message {
                Some(msg) => format!("QUIT :{}", msg),
                None => "QUIT".to_string(),
            },
            Command::Join { channels } => {
                let chans: Vec<&str> = channels.iter().map(|(c, _)| c.as_str()).collect();
                let keys: Vec<&str> = channels
                    .iter()
                    .filter_map(|(_, k)| k.as_deref())
                    .collect();
                if keys.is_empty() {
                    format!("JOIN :{}", chans.join(","))
                } else {
                    format!("JOIN {} :{}", chans.join(","), keys.join(","))
                }
            }
            Command::Part { channels, message } => match message {
                Some(msg) => format!("PART {} :{}", channels.join(","), msg),
                None => format!("PART :{}", channels.join(",")),
            },
            Command::Topic { channel, topic } => match topic {
                Some(t) => format!("TOPIC {} :{}", channel, t),
                None => format!("TOPIC :{}", channel),
            },
            Command::Names { channels } => {
                if channels.is_empty() {
                    "NAMES".to_string()
                } else {
                    format!("NAMES :{}", channels.join(","))
                }
            }
            Command::Kick { channel, user, reason } => match reason {
                Some(r) => format!("KICK {} {} :{}", channel, user, r),
                None => format!("KICK {} :{}", channel, user),
            },
            Command::Invite { nickname, channel } => {
                format!("INVITE {} :{}", nickname, channel)
            }
            Command::Privmsg { target, text } => format!("PRIVMSG {} :{}", target, text),
            Command::Notice { target, text } => format!("NOTICE {} :{}", target, text),
            Command::Ping { token } => format!("PING :{}", token),
            Command::Pong { token } => format!("PONG :{}", token),
            Command::Mode { target, modes, params } => {
                let mut s = format!("MODE {}", target);
                if let Some(m) = modes {
                    s.push(' ');
                    s.push_str(m);
                    for p in params {
                        s.push(' ');
                        s.push_str(p);
                    }
                }
                s
            }
            Command::Cap { subcommand } => match subcommand {
                CapSubcommand::Ls { version, .. } => match version {
                    Some(v) => format!("CAP LS {}", v),
                    None => "CAP LS".to_string(),
                },
                CapSubcommand::List { .. } => "CAP LIST".to_string(),
                CapSubcommand::Req { caps } => format!("CAP REQ :{}", caps),
                CapSubcommand::Ack { caps } => format!("CAP ACK :{}", caps),
                CapSubcommand::Nak { caps } => format!("CAP NAK :{}", caps),
                CapSubcommand::New { caps } => format!("CAP NEW :{}", caps),
                CapSubcommand::Del { caps } => format!("CAP DEL :{}", caps),
                CapSubcommand::End => "CAP END".to_string(),
            },
            Command::Authenticate { data } => format!("AUTHENTICATE {}", data),
            Command::Numeric { code, params } => {
                if params.is_empty() {
                    format!("{:03}", code)
                } else {
                    let last = params.len() - 1;
                    let mut s = format!("{:03}", code);
                    for (i, p) in params.iter().enumerate() {
                        if i == last {
                            s.push_str(&format!(" :{}", p));
                        } else {
                            s.push_str(&format!(" {}", p));
                        }
                    }
                    s
                }
            }
            Command::Raw { command, params } => {
                if params.is_empty() {
                    command.clone()
                } else {
                    let last = params.len() - 1;
                    let mut s = command.clone();
                    for (i, p) in params.iter().enumerate() {
                        if i == last {
                            s.push_str(&format!(" :{}", p));
                        } else {
                            s.push_str(&format!(" {}", p));
                        }
                    }
                    s
                }
            }
        }
    }
}

impl ParsedMessage {
    /// Create a ParsedMessage from an OwnedIrcMessage.
    pub fn from_owned(msg: OwnedIrcMessage) -> Self {
        let server_time = msg
            .tags
            .iter()
            .find(|t| t.key == "time")
            .and_then(|t| t.value.as_deref())
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let command = Command::from_message(&msg);

        ParsedMessage {
            tags: msg.tags,
            prefix: msg.prefix,
            command,
            server_time,
        }
    }

    /// Get the nick from the message prefix, if it's a user prefix.
    pub fn prefix_nick(&self) -> Option<&str> {
        self.prefix.as_ref().and_then(|p| p.nick())
    }

    /// Extract user@host from the prefix, if available.
    pub fn prefix_userhost(&self) -> Option<String> {
        match self.prefix.as_ref()? {
            crate::irc::message::OwnedPrefix::User { user, host, .. } => {
                let u = user.as_deref().unwrap_or("~");
                let h = host.as_deref().unwrap_or("");
                if h.is_empty() {
                    None
                } else {
                    Some(format!("{}@{}", u, h))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::message::OwnedIrcMessage;

    fn make_msg(command: &str, params: &[&str]) -> OwnedIrcMessage {
        OwnedIrcMessage {
            tags: Vec::new(),
            prefix: None,
            command: command.to_string(),
            params: params.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parse_privmsg() {
        let msg = make_msg("PRIVMSG", &["#channel", "Hello world"]);
        let cmd = Command::from_message(&msg);
        assert_eq!(
            cmd,
            Command::Privmsg {
                target: "#channel".to_string(),
                text: "Hello world".to_string(),
            }
        );
    }

    #[test]
    fn parse_numeric() {
        let msg = make_msg("001", &["nick", "Welcome to the network"]);
        let cmd = Command::from_message(&msg);
        assert_eq!(
            cmd,
            Command::Numeric {
                code: 1,
                params: vec!["nick".to_string(), "Welcome to the network".to_string()],
            }
        );
    }

    #[test]
    fn parse_cap_ls() {
        let msg = make_msg("CAP", &["*", "LS", "multi-prefix sasl"]);
        let cmd = Command::from_message(&msg);
        assert_eq!(
            cmd,
            Command::Cap {
                subcommand: CapSubcommand::Ls {
                    version: None,
                    caps: Some("multi-prefix sasl".to_string()),
                }
            }
        );
    }

    #[test]
    fn parse_cap_ack() {
        let msg = make_msg("CAP", &["*", "ACK", "multi-prefix sasl"]);
        let cmd = Command::from_message(&msg);
        assert_eq!(
            cmd,
            Command::Cap {
                subcommand: CapSubcommand::Ack {
                    caps: "multi-prefix sasl".to_string(),
                }
            }
        );
    }

    #[test]
    fn parse_unknown_command() {
        let msg = make_msg("FOOBAR", &["arg1", "arg2"]);
        let cmd = Command::from_message(&msg);
        assert_eq!(
            cmd,
            Command::Raw {
                command: "FOOBAR".to_string(),
                params: vec!["arg1".to_string(), "arg2".to_string()],
            }
        );
    }

    #[test]
    fn parse_join_with_keys() {
        let msg = make_msg("JOIN", &["#a,#b", "key1,key2"]);
        let cmd = Command::from_message(&msg);
        assert_eq!(
            cmd,
            Command::Join {
                channels: vec![
                    ("#a".to_string(), Some("key1".to_string())),
                    ("#b".to_string(), Some("key2".to_string())),
                ],
            }
        );
    }

    #[test]
    fn to_raw_privmsg() {
        let cmd = Command::Privmsg {
            target: "#test".to_string(),
            text: "hello world".to_string(),
        };
        assert_eq!(cmd.to_raw(), "PRIVMSG #test :hello world");
    }

    #[test]
    fn to_raw_join() {
        let cmd = Command::Join {
            channels: vec![("#rust".to_string(), None)],
        };
        assert_eq!(cmd.to_raw(), "JOIN :#rust");
    }

    #[test]
    fn to_raw_cap_req() {
        let cmd = Command::Cap {
            subcommand: CapSubcommand::Req {
                caps: "multi-prefix sasl".to_string(),
            },
        };
        assert_eq!(cmd.to_raw(), "CAP REQ :multi-prefix sasl");
    }

    #[test]
    fn round_trip_via_parse_and_raw() {
        let cmd = Command::Nick {
            nickname: "emilio".to_string(),
        };
        let raw = cmd.to_raw();
        let parsed = crate::irc::parser::parse(&raw).unwrap();
        let owned = OwnedIrcMessage::from(parsed);
        let cmd2 = Command::from_message(&owned);
        assert_eq!(cmd, cmd2);
    }

    #[test]
    fn parsed_message_with_server_time() {
        let msg = OwnedIrcMessage {
            tags: vec![OwnedTag {
                key: "time".to_string(),
                value: Some("2026-03-30T12:00:00Z".to_string()),
            }],
            prefix: Some(OwnedPrefix::User {
                nick: "nick".to_string(),
                user: Some("user".to_string()),
                host: Some("host".to_string()),
            }),
            command: "PRIVMSG".to_string(),
            params: vec!["#channel".to_string(), "hello".to_string()],
        };
        let parsed = ParsedMessage::from_owned(msg);
        assert!(parsed.server_time.is_some());
        assert_eq!(parsed.prefix_nick(), Some("nick"));
    }

    #[test]
    fn parsed_message_without_server_time() {
        let msg = OwnedIrcMessage {
            tags: Vec::new(),
            prefix: None,
            command: "PING".to_string(),
            params: vec!["token".to_string()],
        };
        let parsed = ParsedMessage::from_owned(msg);
        assert!(parsed.server_time.is_none());
        assert_eq!(parsed.prefix_nick(), None);
    }
}
