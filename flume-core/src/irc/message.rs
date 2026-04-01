use std::fmt;

/// A parsed IRC message with borrowed references into the input buffer.
///
/// Supports both IRCv3 (with message tags) and legacy RFC 1459 messages.
/// The parser is zero-copy: all string fields are slices of the original input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrcMessage<'a> {
    /// IRCv3 message tags. Empty if no tags present.
    pub tags: Vec<Tag<'a>>,
    /// Message prefix (nick!user@host or servername). None if absent.
    pub prefix: Option<Prefix<'a>>,
    /// The IRC command (e.g., "PRIVMSG", "001", "CAP").
    pub command: &'a str,
    /// Command parameters.
    pub params: Vec<&'a str>,
}

/// An IRCv3 message tag (key=value pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag<'a> {
    pub key: &'a str,
    pub value: Option<&'a str>,
}

/// The source prefix of an IRC message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prefix<'a> {
    /// A server name (no `!` or `@` in the prefix).
    Server(&'a str),
    /// A user prefix: nick, optional user, optional host.
    User {
        nick: &'a str,
        user: Option<&'a str>,
        host: Option<&'a str>,
    },
}

// --- Owned variants ---

/// Owned version of `IrcMessage` that doesn't borrow from the input buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedIrcMessage {
    pub tags: Vec<OwnedTag>,
    pub prefix: Option<OwnedPrefix>,
    pub command: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedTag {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedPrefix {
    Server(String),
    User {
        nick: String,
        user: Option<String>,
        host: Option<String>,
    },
}

// --- Conversions ---

impl<'a> From<IrcMessage<'a>> for OwnedIrcMessage {
    fn from(msg: IrcMessage<'a>) -> Self {
        OwnedIrcMessage {
            tags: msg.tags.into_iter().map(OwnedTag::from).collect(),
            prefix: msg.prefix.map(OwnedPrefix::from),
            command: msg.command.to_owned(),
            params: msg.params.into_iter().map(|s| s.to_owned()).collect(),
        }
    }
}

impl<'a> From<Tag<'a>> for OwnedTag {
    fn from(tag: Tag<'a>) -> Self {
        OwnedTag {
            key: tag.key.to_owned(),
            value: tag.value.map(|v| v.to_owned()),
        }
    }
}

impl<'a> From<Prefix<'a>> for OwnedPrefix {
    fn from(prefix: Prefix<'a>) -> Self {
        match prefix {
            Prefix::Server(s) => OwnedPrefix::Server(s.to_owned()),
            Prefix::User { nick, user, host } => OwnedPrefix::User {
                nick: nick.to_owned(),
                user: user.map(|s| s.to_owned()),
                host: host.map(|s| s.to_owned()),
            },
        }
    }
}

impl OwnedPrefix {
    /// Extract the nick from a user prefix, or None for server prefixes.
    pub fn nick(&self) -> Option<&str> {
        match self {
            OwnedPrefix::User { nick, .. } => Some(nick),
            OwnedPrefix::Server(_) => None,
        }
    }
}

impl<'a> Prefix<'a> {
    /// Extract the nick from a user prefix, or None for server prefixes.
    pub fn nick(&self) -> Option<&'a str> {
        match self {
            Prefix::User { nick, .. } => Some(nick),
            Prefix::Server(_) => None,
        }
    }
}

// --- Display implementations for wire format ---

impl fmt::Display for IrcMessage<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.tags.is_empty() {
            f.write_str("@")?;
            for (i, tag) in self.tags.iter().enumerate() {
                if i > 0 {
                    f.write_str(";")?;
                }
                write!(f, "{}", tag)?;
            }
            f.write_str(" ")?;
        }
        if let Some(ref prefix) = self.prefix {
            write!(f, ":{} ", prefix)?;
        }
        f.write_str(self.command)?;
        for (i, param) in self.params.iter().enumerate() {
            let is_last = i == self.params.len() - 1;
            if is_last {
                write!(f, " :{}", param)?;
            } else {
                write!(f, " {}", param)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for OwnedIrcMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.tags.is_empty() {
            f.write_str("@")?;
            for (i, tag) in self.tags.iter().enumerate() {
                if i > 0 {
                    f.write_str(";")?;
                }
                write!(f, "{}", tag)?;
            }
            f.write_str(" ")?;
        }
        if let Some(ref prefix) = self.prefix {
            write!(f, ":{} ", prefix)?;
        }
        f.write_str(&self.command)?;
        for (i, param) in self.params.iter().enumerate() {
            let is_last = i == self.params.len() - 1;
            if is_last {
                write!(f, " :{}", param)?;
            } else {
                write!(f, " {}", param)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Tag<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key)?;
        if let Some(value) = self.value {
            f.write_str("=")?;
            f.write_str(value)?;
        }
        Ok(())
    }
}

impl fmt::Display for OwnedTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key)?;
        if let Some(ref value) = self.value {
            f.write_str("=")?;
            f.write_str(value)?;
        }
        Ok(())
    }
}

impl fmt::Display for Prefix<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Prefix::Server(s) => f.write_str(s),
            Prefix::User { nick, user, host } => {
                f.write_str(nick)?;
                if let Some(user) = user {
                    write!(f, "!{}", user)?;
                }
                if let Some(host) = host {
                    write!(f, "@{}", host)?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for OwnedPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OwnedPrefix::Server(s) => f.write_str(s),
            OwnedPrefix::User { nick, user, host } => {
                f.write_str(nick)?;
                if let Some(user) = user {
                    write!(f, "!{}", user)?;
                }
                if let Some(host) = host {
                    write!(f, "@{}", host)?;
                }
                Ok(())
            }
        }
    }
}
