//! Outgoing command surface — stubs in M1, implementation in M5.
//! All methods push into the per-server command channel and return
//! immediately. Errors are limited to validation (unknown server,
//! invalid target); wire-level failures arrive later as `Event::Error`.

use crate::{buffers::BufferRef, error::FlumeError, FlumeClient};

#[uniffi::export]
impl FlumeClient {
    pub fn send_message(&self, _buffer_ref: BufferRef, _text: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn send_action(&self, _buffer_ref: BufferRef, _text: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn send_notice(
        &self,
        _server: String,
        _target: String,
        _text: String,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn join_channel(
        &self,
        _server: String,
        _channel: String,
        _key: Option<String>,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn part_channel(
        &self,
        _server: String,
        _channel: String,
        _message: Option<String>,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn change_nick(&self, _server: String, _nick: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn set_topic(
        &self,
        _server: String,
        _channel: String,
        _topic: Option<String>,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn kick(
        &self,
        _server: String,
        _channel: String,
        _nick: String,
        _reason: Option<String>,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn set_mode(
        &self,
        _server: String,
        _target: String,
        _modes: String,
        _params: Vec<String>,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn whois(&self, _server: String, _nick: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn invite(
        &self,
        _server: String,
        _channel: String,
        _nick: String,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn send_raw(&self, _server: String, _line: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }
}
