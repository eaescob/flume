//! Server/network surface — stubs in M1, implementation in M3.

use crate::{error::FlumeError, FlumeClient};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Registering,
    Connected,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum AuthMethod {
    None,
    Sasl,
    NickServ,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BouncerType {
    None,
    Znc,
    Soju,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AuthConfig {
    pub method: AuthMethod,
    pub username: Option<String>,
    /// May be a literal value or a `${vault_key}` reference.
    pub password: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct NetworkConfig {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub tls: bool,
    pub tls_accept_invalid_certs: bool,
    pub nick: Option<String>,
    pub username: Option<String>,
    pub realname: Option<String>,
    pub autojoin: Vec<String>,
    pub auth: AuthConfig,
    pub autoconnect: bool,
    pub bouncer: BouncerType,
    pub password: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ServerInfo {
    pub name: String,
    pub state: ConnectionState,
    pub our_nick: Option<String>,
    pub user_modes: String,
    pub capabilities: Vec<String>,
}

#[uniffi::export]
impl FlumeClient {
    pub fn add_network(&self, _config: NetworkConfig) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn remove_network(&self, _name: String) {}

    pub fn list_networks(&self) -> Vec<NetworkConfig> {
        Vec::new()
    }

    /// Flushes the in-memory network list to disk. Mirrors the TUI's explicit
    /// `/save` convention — mutations are not auto-persisted.
    pub fn save_networks(&self) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn connect_network(&self, _name: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn disconnect_network(&self, _name: String, _reason: Option<String>) {}

    pub fn list_servers(&self) -> Vec<ServerInfo> {
        Vec::new()
    }
}
