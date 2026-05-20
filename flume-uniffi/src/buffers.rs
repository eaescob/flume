//! Buffer surface — stubs in M1, implementation in M4. Per the v1 plan,
//! Rust owns protocol state (topic, modes, nick list, unread counts) and
//! Swift owns rendered message history.

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

#[uniffi::export]
impl FlumeClient {
    pub fn list_buffers(&self, _server: String) -> Vec<BufferInfo> {
        Vec::new()
    }

    pub fn buffer_info(&self, _buffer_ref: BufferRef) -> Option<BufferInfo> {
        None
    }

    pub fn mark_read(&self, _buffer_ref: BufferRef) {}

    pub fn close_buffer(&self, _buffer_ref: BufferRef) {}
}
