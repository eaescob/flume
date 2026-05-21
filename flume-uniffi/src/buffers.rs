//! Buffer state mirror. Per the M3/M4 plan, Rust owns protocol state
//! (kind/topic/modes/nicks/unread counters); Swift owns rendered message
//! history.
//!
//! `ServerBuffers` is the per-server map; `BufferState` is one row. The
//! FFI surface exposes `BufferInfo` snapshots only — internal state stays
//! behind the Mutex.

use std::collections::HashMap;

use crate::FlumeClient;

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BufferKind {
    Server,
    Channel,
    PrivateMessage,
    /// Routed by an snotice rule.
    Snotice,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BufferRef {
    pub server: String,
    /// `""` for the server buffer; channel name or PM nick otherwise.
    pub target: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct NickEntry {
    pub nick: String,
    /// Highest channel-mode prefix character: `"+"`, `"@"`, `"&"`, etc.
    /// Empty string when no mode is held.
    pub prefix: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BufferInfo {
    pub buffer_ref: BufferRef,
    pub kind: BufferKind,
    pub topic: Option<String>,
    pub channel_modes: Option<String>,
    pub nicks: Vec<NickEntry>,
    pub unread: u32,
    pub highlights: u32,
}

// ───────────────────────── internal state ─────────────────────────

#[derive(Debug, Default)]
pub(crate) struct BufferState {
    pub kind: BufferKind,
    pub topic: Option<String>,
    pub channel_modes: Option<String>,
    pub nicks: Vec<NickEntry>,
    pub unread: u32,
    pub highlights: u32,
}

impl Default for BufferKind {
    fn default() -> Self {
        BufferKind::Server
    }
}

#[derive(Debug, Default)]
pub(crate) struct ServerBuffers {
    pub map: HashMap<String, BufferState>,
}

impl ServerBuffers {
    pub(crate) fn ensure(&mut self, target: &str, kind: BufferKind) {
        let key = normalize_target(target);
        self.map.entry(key).or_insert_with(|| BufferState {
            kind,
            ..Default::default()
        });
    }

    pub(crate) fn remove(&mut self, target: &str) {
        let key = normalize_target(target);
        self.map.remove(&key);
    }

    pub(crate) fn set_topic(&mut self, channel: &str, topic: Option<String>) {
        let key = normalize_target(channel);
        if let Some(buf) = self.map.get_mut(&key) {
            buf.topic = topic;
        }
    }

    pub(crate) fn set_channel_modes(&mut self, channel: &str, modes: Option<String>) {
        let key = normalize_target(channel);
        if let Some(buf) = self.map.get_mut(&key) {
            buf.channel_modes = modes;
        }
    }

    pub(crate) fn set_nicks(&mut self, channel: &str, nicks: Vec<NickEntry>) {
        let key = normalize_target(channel);
        if let Some(buf) = self.map.get_mut(&key) {
            buf.nicks = nicks;
            sort_nicks(&mut buf.nicks);
        }
    }

    pub(crate) fn add_nick(&mut self, channel: &str, nick: &str, prefix: &str) {
        let key = normalize_target(channel);
        if let Some(buf) = self.map.get_mut(&key) {
            if !buf.nicks.iter().any(|n| n.nick == nick) {
                buf.nicks.push(NickEntry {
                    nick: nick.to_string(),
                    prefix: prefix.to_string(),
                });
                sort_nicks(&mut buf.nicks);
            }
        }
    }

    pub(crate) fn remove_nick(&mut self, channel: &str, nick: &str) {
        let key = normalize_target(channel);
        if let Some(buf) = self.map.get_mut(&key) {
            buf.nicks.retain(|n| n.nick != nick);
        }
    }

    pub(crate) fn remove_nick_everywhere(&mut self, nick: &str) {
        for buf in self.map.values_mut() {
            buf.nicks.retain(|n| n.nick != nick);
        }
    }

    pub(crate) fn rename_nick(&mut self, old: &str, new: &str) {
        for buf in self.map.values_mut() {
            for n in buf.nicks.iter_mut() {
                if n.nick == old {
                    n.nick = new.to_string();
                }
            }
            sort_nicks(&mut buf.nicks);
        }
    }

    pub(crate) fn apply_mode_to_nicks(
        &mut self,
        channel: &str,
        mode_str: &str,
        params: &[String],
    ) {
        // Channel-user modes that change nick prefix: o (op, @), v (voice, +),
        // h (half-op, %), a (admin, &), q (owner, ~). Skipping ban-list and
        // other non-user modes; the TUI has a fuller version we can lift later.
        let key = normalize_target(channel);
        let Some(buf) = self.map.get_mut(&key) else {
            return;
        };
        let mut adding = true;
        let mut param_idx = 0;
        for ch in mode_str.chars() {
            match ch {
                '+' => adding = true,
                '-' => adding = false,
                'o' | 'v' | 'h' | 'a' | 'q' => {
                    if let Some(nick) = params.get(param_idx) {
                        let prefix_char = match ch {
                            'q' => "~",
                            'a' => "&",
                            'o' => "@",
                            'h' => "%",
                            'v' => "+",
                            _ => "",
                        };
                        for n in buf.nicks.iter_mut() {
                            if n.nick == *nick {
                                n.prefix = if adding { prefix_char.to_string() } else { String::new() };
                            }
                        }
                        param_idx += 1;
                    }
                }
                'b' | 'e' | 'I' | 'k' => {
                    param_idx += 1;
                }
                'l' if adding => {
                    param_idx += 1;
                }
                _ => {}
            }
        }
        sort_nicks(&mut buf.nicks);
    }

    pub(crate) fn increment_unread(&mut self, target: &str, is_highlight: bool) {
        let key = normalize_target(target);
        if let Some(buf) = self.map.get_mut(&key) {
            buf.unread += 1;
            if is_highlight {
                buf.highlights += 1;
            }
        }
    }

    pub(crate) fn mark_read(&mut self, target: &str) {
        let key = normalize_target(target);
        if let Some(buf) = self.map.get_mut(&key) {
            buf.unread = 0;
            buf.highlights = 0;
        }
    }

    pub(crate) fn snapshot(&self, server: &str) -> Vec<BufferInfo> {
        self.map
            .iter()
            .map(|(target, buf)| BufferInfo {
                buffer_ref: BufferRef {
                    server: server.to_string(),
                    target: target.clone(),
                },
                kind: buf.kind.clone(),
                topic: buf.topic.clone(),
                channel_modes: buf.channel_modes.clone(),
                nicks: buf.nicks.clone(),
                unread: buf.unread,
                highlights: buf.highlights,
            })
            .collect()
    }
}

pub(crate) fn normalize_target(name: &str) -> String {
    if is_channel(name) {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

pub(crate) fn is_channel(name: &str) -> bool {
    matches!(name.as_bytes().first(), Some(b'#' | b'&' | b'+' | b'!'))
}

fn prefix_priority(p: &str) -> u8 {
    match p {
        "~" => 5,
        "&" => 4,
        "@" => 3,
        "%" => 2,
        "+" => 1,
        _ => 0,
    }
}

fn sort_nicks(nicks: &mut [NickEntry]) {
    nicks.sort_by(|a, b| {
        prefix_priority(&b.prefix)
            .cmp(&prefix_priority(&a.prefix))
            .then_with(|| a.nick.to_lowercase().cmp(&b.nick.to_lowercase()))
    });
}

/// Parse a NAMES-list nick like `@alice`, `+bob`, `~carol`.
pub(crate) fn parse_prefixed_nick(s: &str) -> NickEntry {
    let bytes = s.as_bytes();
    let prefix_len = bytes
        .iter()
        .take_while(|b| matches!(b, b'~' | b'&' | b'@' | b'%' | b'+'))
        .count();
    let prefix = if prefix_len > 0 {
        s[..1].to_string() // use the highest-priority prefix (first char)
    } else {
        String::new()
    };
    let nick = s[prefix_len..].to_string();
    NickEntry { nick, prefix }
}

// ───────────────────────── FFI surface ─────────────────────────

#[uniffi::export]
impl FlumeClient {
    pub fn list_buffers(&self, server: String) -> Vec<BufferInfo> {
        let state = self.state.lock().expect("state poisoned");
        let Some(handle) = state.servers.get(&server) else {
            return Vec::new();
        };
        let snapshot = handle
            .buffers
            .lock()
            .expect("buffers poisoned")
            .snapshot(&server);
        snapshot
    }

    pub fn buffer_info(&self, buffer_ref: BufferRef) -> Option<BufferInfo> {
        let state = self.state.lock().expect("state poisoned");
        let handle = state.servers.get(&buffer_ref.server)?;
        let buffers = handle.buffers.lock().expect("buffers poisoned");
        let key = normalize_target(&buffer_ref.target);
        buffers.map.get(&key).map(|buf| BufferInfo {
            buffer_ref: BufferRef {
                server: buffer_ref.server.clone(),
                target: key,
            },
            kind: buf.kind.clone(),
            topic: buf.topic.clone(),
            channel_modes: buf.channel_modes.clone(),
            nicks: buf.nicks.clone(),
            unread: buf.unread,
            highlights: buf.highlights,
        })
    }

    pub fn mark_read(&self, buffer_ref: BufferRef) {
        let state = self.state.lock().expect("state poisoned");
        let Some(handle) = state.servers.get(&buffer_ref.server) else {
            return;
        };
        handle
            .buffers
            .lock()
            .expect("buffers poisoned")
            .mark_read(&buffer_ref.target);
    }

    /// Drop the in-memory buffer state. Does NOT send PART to the server —
    /// callers wanting a clean leave should `part_channel(...)` first.
    pub fn close_buffer(&self, buffer_ref: BufferRef) {
        let state = self.state.lock().expect("state poisoned");
        let Some(handle) = state.servers.get(&buffer_ref.server) else {
            return;
        };
        handle
            .buffers
            .lock()
            .expect("buffers poisoned")
            .remove(&buffer_ref.target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nick_sort_prefix_priority() {
        let mut buf = BufferState::default();
        buf.nicks = vec![
            NickEntry { nick: "alice".into(), prefix: "".into() },
            NickEntry { nick: "bob".into(), prefix: "@".into() },
            NickEntry { nick: "carol".into(), prefix: "+".into() },
            NickEntry { nick: "dave".into(), prefix: "~".into() },
        ];
        sort_nicks(&mut buf.nicks);
        let order: Vec<&str> = buf.nicks.iter().map(|n| n.nick.as_str()).collect();
        assert_eq!(order, vec!["dave", "bob", "carol", "alice"]);
    }

    #[test]
    fn names_parses_prefixes() {
        assert_eq!(parse_prefixed_nick("alice").prefix, "");
        assert_eq!(parse_prefixed_nick("@bob").prefix, "@");
        assert_eq!(parse_prefixed_nick("+carol").nick, "carol");
        assert_eq!(parse_prefixed_nick("~dave").prefix, "~");
    }

    #[test]
    fn channel_normalization_lowercases() {
        assert_eq!(normalize_target("#Flume"), "#flume");
        assert_eq!(normalize_target("alice"), "alice");
    }

    #[test]
    fn apply_mode_op_voice() {
        let mut buf = BufferState {
            kind: BufferKind::Channel,
            ..Default::default()
        };
        buf.nicks = vec![
            NickEntry { nick: "alice".into(), prefix: "".into() },
            NickEntry { nick: "bob".into(), prefix: "".into() },
        ];
        let mut bufs = ServerBuffers::default();
        bufs.map.insert("#chan".into(), buf);
        bufs.apply_mode_to_nicks(
            "#chan",
            "+ov",
            &["alice".into(), "bob".into()],
        );
        let nicks = &bufs.map.get("#chan").unwrap().nicks;
        let alice = nicks.iter().find(|n| n.nick == "alice").unwrap();
        let bob = nicks.iter().find(|n| n.nick == "bob").unwrap();
        assert_eq!(alice.prefix, "@");
        assert_eq!(bob.prefix, "+");
    }
}
