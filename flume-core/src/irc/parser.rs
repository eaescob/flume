use std::borrow::Cow;

use crate::irc::message::{IrcMessage, Prefix, Tag};

/// Errors that can occur when parsing an IRC message.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("empty message")]
    Empty,
    #[error("missing command")]
    MissingCommand,
}

/// Parse a single IRC message line.
///
/// Input should NOT include the trailing `\r\n`.
/// Handles both IRCv3 messages (with `@tags`) and legacy RFC 1459 messages.
///
/// Grammar:
/// ```text
/// message = ['@' tags SPACE] [':' prefix SPACE] command [params]
/// tags    = tag [';' tag]*
/// tag     = key ['=' value]
/// params  = (SPACE middle)* [SPACE ':' trailing]
/// ```
pub fn parse(input: &str) -> Result<IrcMessage<'_>, ParseError> {
    let input = input.trim_end_matches(|c| c == '\r' || c == '\n');
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let mut rest = input;

    // Parse tags
    let tags = if rest.starts_with('@') {
        let space = rest.find(' ').ok_or(ParseError::MissingCommand)?;
        let tags_str = &rest[1..space];
        rest = &rest[space + 1..];
        parse_tags(tags_str)
    } else {
        Vec::new()
    };

    // Skip leading spaces (shouldn't happen in valid messages, but be defensive)
    rest = rest.trim_start_matches(' ');

    // Parse prefix
    let prefix = if rest.starts_with(':') {
        let space = rest.find(' ').ok_or(ParseError::MissingCommand)?;
        let prefix_str = &rest[1..space];
        rest = &rest[space + 1..];
        Some(parse_prefix(prefix_str))
    } else {
        None
    };

    rest = rest.trim_start_matches(' ');

    // Parse command
    if rest.is_empty() {
        return Err(ParseError::MissingCommand);
    }

    let (command, remainder) = match rest.find(' ') {
        Some(pos) => (&rest[..pos], &rest[pos + 1..]),
        None => (rest, ""),
    };

    // Parse params
    let params = parse_params(remainder);

    Ok(IrcMessage {
        tags,
        prefix,
        command,
        params,
    })
}

/// Parse the tags section (everything after `@` and before the first space).
fn parse_tags(input: &str) -> Vec<Tag<'_>> {
    if input.is_empty() {
        return Vec::new();
    }
    input
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|tag_str| {
            match tag_str.find('=') {
                Some(pos) => Tag {
                    key: &tag_str[..pos],
                    value: Some(&tag_str[pos + 1..]),
                },
                None => Tag {
                    key: tag_str,
                    value: None,
                },
            }
        })
        .collect()
}

/// Parse a prefix string (everything after `:` and before the first space).
///
/// If it contains `!` or `@`, it's a user prefix (nick!user@host).
/// Otherwise it's a server prefix.
fn parse_prefix(input: &str) -> Prefix<'_> {
    let bang = input.find('!');
    let at = input.find('@');

    match (bang, at) {
        (Some(b), Some(a)) if b < a => Prefix::User {
            nick: &input[..b],
            user: Some(&input[b + 1..a]),
            host: Some(&input[a + 1..]),
        },
        (Some(b), None) => Prefix::User {
            nick: &input[..b],
            user: Some(&input[b + 1..]),
            host: None,
        },
        (None, Some(a)) => Prefix::User {
            nick: &input[..a],
            user: None,
            host: Some(&input[a + 1..]),
        },
        _ => {
            // If it contains a dot, it's likely a server name.
            // If no dot, treat as a nick-only user prefix.
            if input.contains('.') {
                Prefix::Server(input)
            } else {
                Prefix::User {
                    nick: input,
                    user: None,
                    host: None,
                }
            }
        }
    }
}

/// Parse IRC message parameters.
///
/// Parameters are space-separated. If a parameter starts with `:`,
/// the rest of the line (including spaces) is the trailing parameter.
fn parse_params(input: &str) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut params = Vec::new();
    let mut rest = input;

    loop {
        if rest.is_empty() {
            break;
        }

        if rest.starts_with(':') {
            // Trailing parameter: everything after the `:` is one param
            params.push(&rest[1..]);
            break;
        }

        match rest.find(' ') {
            Some(pos) => {
                params.push(&rest[..pos]);
                rest = &rest[pos + 1..];
            }
            None => {
                params.push(rest);
                break;
            }
        }
    }

    params
}

/// Unescape an IRCv3 tag value.
///
/// Escape sequences:
/// - `\:` → `;`
/// - `\s` → ` ` (space)
/// - `\\` → `\`
/// - `\r` → CR
/// - `\n` → LF
/// - `\x` → `x` (any other escaped char is just the char itself)
pub fn unescape_tag_value(input: &str) -> Cow<'_, str> {
    if !input.contains('\\') {
        return Cow::Borrowed(input);
    }

    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(':') => result.push(';'),
                Some('s') => result.push(' '),
                Some('\\') => result.push('\\'),
                Some('r') => result.push('\r'),
                Some('n') => result.push('\n'),
                Some(other) => result.push(other),
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    Cow::Owned(result)
}

/// Escape a string for use as an IRCv3 tag value.
pub fn escape_tag_value(input: &str) -> Cow<'_, str> {
    if !input.contains(|c| c == ';' || c == ' ' || c == '\\' || c == '\r' || c == '\n') {
        return Cow::Borrowed(input);
    }

    let mut result = String::with_capacity(input.len() + 4);
    for c in input.chars() {
        match c {
            ';' => result.push_str("\\:"),
            ' ' => result.push_str("\\s"),
            '\\' => result.push_str("\\\\"),
            '\r' => result.push_str("\\r"),
            '\n' => result.push_str("\\n"),
            _ => result.push(c),
        }
    }

    Cow::Owned(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::message::Prefix;

    #[test]
    fn parse_simple_ping() {
        let msg = parse("PING :irc.libera.chat").unwrap();
        assert!(msg.tags.is_empty());
        assert!(msg.prefix.is_none());
        assert_eq!(msg.command, "PING");
        assert_eq!(msg.params, vec!["irc.libera.chat"]);
    }

    #[test]
    fn parse_prefixed_privmsg() {
        let msg = parse(":nick!user@host PRIVMSG #channel :Hello world").unwrap();
        assert!(msg.tags.is_empty());
        assert_eq!(
            msg.prefix,
            Some(Prefix::User {
                nick: "nick",
                user: Some("user"),
                host: Some("host"),
            })
        );
        assert_eq!(msg.command, "PRIVMSG");
        assert_eq!(msg.params, vec!["#channel", "Hello world"]);
    }

    #[test]
    fn parse_numeric_reply() {
        let msg = parse(":irc.libera.chat 001 nick :Welcome to the network").unwrap();
        assert_eq!(msg.prefix, Some(Prefix::Server("irc.libera.chat")));
        assert_eq!(msg.command, "001");
        assert_eq!(msg.params, vec!["nick", "Welcome to the network"]);
    }

    #[test]
    fn parse_message_with_tags() {
        let msg = parse(
            "@time=2026-03-30T12:00:00Z;account=emilio :nick!user@host PRIVMSG #channel :hello",
        )
        .unwrap();
        assert_eq!(msg.tags.len(), 2);
        assert_eq!(msg.tags[0].key, "time");
        assert_eq!(msg.tags[0].value, Some("2026-03-30T12:00:00Z"));
        assert_eq!(msg.tags[1].key, "account");
        assert_eq!(msg.tags[1].value, Some("emilio"));
        assert_eq!(msg.command, "PRIVMSG");
        assert_eq!(msg.params, vec!["#channel", "hello"]);
    }

    #[test]
    fn parse_tags_with_no_value() {
        let msg = parse("@draft/reply;+example :nick PRIVMSG #test :msg").unwrap();
        assert_eq!(msg.tags.len(), 2);
        assert_eq!(msg.tags[0].key, "draft/reply");
        assert_eq!(msg.tags[0].value, None);
        assert_eq!(msg.tags[1].key, "+example");
        assert_eq!(msg.tags[1].value, None);
    }

    #[test]
    fn parse_server_prefix_motd() {
        let msg = parse(":irc.libera.chat 372 nick :- Message of the day").unwrap();
        assert_eq!(msg.prefix, Some(Prefix::Server("irc.libera.chat")));
        assert_eq!(msg.command, "372");
        assert_eq!(msg.params, vec!["nick", "- Message of the day"]);
    }

    #[test]
    fn parse_no_prefix_no_tags() {
        let msg = parse("CAP LS 302").unwrap();
        assert!(msg.tags.is_empty());
        assert!(msg.prefix.is_none());
        assert_eq!(msg.command, "CAP");
        assert_eq!(msg.params, vec!["LS", "302"]);
    }

    #[test]
    fn parse_trailing_with_colons() {
        let msg = parse(":nick PRIVMSG #test :hello: world: test").unwrap();
        assert_eq!(msg.params, vec!["#test", "hello: world: test"]);
    }

    #[test]
    fn parse_empty_trailing() {
        let msg = parse(":nick PRIVMSG #test :").unwrap();
        assert_eq!(msg.params, vec!["#test", ""]);
    }

    #[test]
    fn parse_many_params() {
        let msg = parse("CMD a b c d e f g h i j k l m n :trailing").unwrap();
        assert_eq!(msg.command, "CMD");
        assert_eq!(
            msg.params,
            vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "trailing"]
        );
    }

    #[test]
    fn parse_command_only() {
        let msg = parse("QUIT").unwrap();
        assert_eq!(msg.command, "QUIT");
        assert!(msg.params.is_empty());
    }

    #[test]
    fn parse_nick_only_prefix() {
        let msg = parse(":nick QUIT :Leaving").unwrap();
        assert_eq!(
            msg.prefix,
            Some(Prefix::User {
                nick: "nick",
                user: None,
                host: None,
            })
        );
        assert_eq!(msg.command, "QUIT");
        assert_eq!(msg.params, vec!["Leaving"]);
    }

    #[test]
    fn parse_nick_at_host_no_user() {
        let msg = parse(":nick@host PRIVMSG #test :hi").unwrap();
        assert_eq!(
            msg.prefix,
            Some(Prefix::User {
                nick: "nick",
                user: None,
                host: Some("host"),
            })
        );
    }

    #[test]
    fn parse_strips_trailing_crlf() {
        let msg = parse("PING :server\r\n").unwrap();
        assert_eq!(msg.command, "PING");
        assert_eq!(msg.params, vec!["server"]);
    }

    #[test]
    fn parse_error_empty() {
        assert_eq!(parse(""), Err(ParseError::Empty));
        assert_eq!(parse("\r\n"), Err(ParseError::Empty));
    }

    #[test]
    fn parse_error_tags_only() {
        assert_eq!(parse("@tag=value"), Err(ParseError::MissingCommand));
    }

    #[test]
    fn parse_error_prefix_only() {
        assert_eq!(parse(":prefix"), Err(ParseError::MissingCommand));
    }

    #[test]
    fn display_round_trip_simple() {
        let input = "PING :irc.libera.chat";
        let msg = parse(input).unwrap();
        assert_eq!(msg.to_string(), input);
    }

    #[test]
    fn display_round_trip_prefixed() {
        let input = ":nick!user@host PRIVMSG #channel :Hello world";
        let msg = parse(input).unwrap();
        assert_eq!(msg.to_string(), input);
    }

    #[test]
    fn display_round_trip_tags() {
        let input = "@time=2026-03-30T12:00:00Z;account=emilio :nick!user@host PRIVMSG #channel :hello";
        let msg = parse(input).unwrap();
        assert_eq!(msg.to_string(), input);
    }

    #[test]
    fn display_round_trip_no_trailing() {
        // Display always uses `:` prefix on last param (safe for wire protocol)
        let input = "CAP LS 302";
        let msg = parse(input).unwrap();
        assert_eq!(msg.to_string(), "CAP LS :302");
    }

    #[test]
    fn unescape_tag_values() {
        assert_eq!(unescape_tag_value("hello"), "hello");
        assert_eq!(unescape_tag_value("hello\\sworld"), "hello world");
        assert_eq!(unescape_tag_value("semi\\:colon"), "semi;colon");
        assert_eq!(unescape_tag_value("back\\\\slash"), "back\\slash");
        assert_eq!(unescape_tag_value("cr\\rlf\\n"), "cr\rlf\n");
        assert_eq!(unescape_tag_value("unknown\\x"), "unknownx");
        assert_eq!(unescape_tag_value("trailing\\"), "trailing\\");
    }

    #[test]
    fn escape_tag_values() {
        assert_eq!(escape_tag_value("hello"), "hello");
        assert_eq!(escape_tag_value("hello world"), "hello\\sworld");
        assert_eq!(escape_tag_value("semi;colon"), "semi\\:colon");
        assert_eq!(escape_tag_value("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn escape_unescape_round_trip() {
        let original = "hello; world\\test\r\n";
        let escaped = escape_tag_value(original);
        let unescaped = unescape_tag_value(&escaped);
        assert_eq!(unescaped, original);
    }

    #[test]
    fn owned_conversion() {
        let msg = parse("@tag=val :nick!user@host PRIVMSG #ch :hello").unwrap();
        let owned = crate::irc::message::OwnedIrcMessage::from(msg.clone());
        assert_eq!(owned.command, "PRIVMSG");
        assert_eq!(owned.params, vec!["#ch", "hello"]);
        assert_eq!(owned.tags.len(), 1);
        assert_eq!(owned.tags[0].key, "tag");
        assert_eq!(owned.tags[0].value, Some("val".to_string()));
        assert!(matches!(
            owned.prefix,
            Some(crate::irc::message::OwnedPrefix::User { ref nick, .. }) if nick == "nick"
        ));
    }

    #[test]
    fn parse_cap_ls_response() {
        let msg = parse(":server CAP * LS :multi-prefix sasl server-time").unwrap();
        assert_eq!(msg.command, "CAP");
        assert_eq!(msg.params, vec!["*", "LS", "multi-prefix sasl server-time"]);
    }

    #[test]
    fn parse_legacy_rfc1459_message() {
        // A typical RFC 1459 message with no tags, just prefix + command + params
        let msg = parse(":irc.example.com 433 * nick :Nickname is already in use").unwrap();
        assert!(msg.tags.is_empty());
        assert_eq!(msg.prefix, Some(Prefix::Server("irc.example.com")));
        assert_eq!(msg.command, "433");
        assert_eq!(msg.params, vec!["*", "nick", "Nickname is already in use"]);
    }
}
