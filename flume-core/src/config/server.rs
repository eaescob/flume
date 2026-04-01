use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-server configuration, loaded from individual TOML files.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub server: ServerConnectionConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub channels: ChannelConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConnectionConfig {
    pub name: String,
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub tls: bool,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub method: AuthMethod,
    #[serde(default)]
    pub sasl_mechanism: SaslMechanism,
    #[serde(default)]
    pub sasl_username: String,
    /// A vault secret reference (e.g., "${libera_pass}") or plain text.
    #[serde(default)]
    pub sasl_password: Option<String>,
    /// Path to client certificate for EXTERNAL auth.
    #[serde(default)]
    pub client_cert: Option<String>,
    /// NickServ password (vault reference or plain text).
    #[serde(default)]
    pub nickserv_password: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig {
            method: AuthMethod::None,
            sasl_mechanism: SaslMechanism::Plain,
            sasl_username: String::new(),
            sasl_password: None,
            client_cert: None,
            nickserv_password: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    Sasl,
    Nickserv,
    #[default]
    None,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum SaslMechanism {
    #[default]
    #[serde(rename = "PLAIN")]
    Plain,
    #[serde(rename = "SCRAM-SHA-256")]
    ScramSha256,
    #[serde(rename = "EXTERNAL")]
    External,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct IdentityConfig {
    #[serde(default)]
    pub nick: Option<String>,
    #[serde(default)]
    pub alt_nicks: Vec<String>,
    #[serde(default)]
    pub realname: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChannelConfig {
    #[serde(default)]
    pub autojoin: Vec<String>,
    #[serde(default)]
    pub keys: HashMap<String, String>,
}

/// Bouncer type for IRC connections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BouncerType {
    #[default]
    None,
    Znc,
    Soju,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdvancedConfig {
    #[serde(default = "default_encoding")]
    pub encoding: String,
    #[serde(default = "default_flood_delay")]
    pub flood_delay_ms: u64,
    #[serde(default = "default_reconnect_attempts")]
    pub reconnect_attempts: u32,
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_ms: u64,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        AdvancedConfig {
            encoding: default_encoding(),
            flood_delay_ms: default_flood_delay(),
            reconnect_attempts: default_reconnect_attempts(),
            reconnect_delay_ms: default_reconnect_delay(),
        }
    }
}

fn default_port() -> u16 { 6697 }
fn default_true() -> bool { true }
fn default_encoding() -> String { "utf-8".to_string() }
fn default_flood_delay() -> u64 { 500 }
fn default_reconnect_attempts() -> u32 { 10 }
fn default_reconnect_delay() -> u64 { 5000 }

// --- irc.toml flat format ---

/// Top-level irc.toml file containing all configured networks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrcConfig {
    #[serde(default, rename = "network")]
    pub networks: Vec<NetworkEntry>,
}

/// A single network entry in irc.toml — flat structure for easy editing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEntry {
    pub name: String,
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub tls: bool,
    #[serde(default)]
    pub auth_method: AuthMethod,
    #[serde(default)]
    pub sasl_mechanism: SaslMechanism,
    #[serde(default)]
    pub sasl_username: String,
    #[serde(default)]
    pub sasl_password: Option<String>,
    #[serde(default)]
    pub nickserv_password: Option<String>,
    #[serde(default)]
    pub nick: Option<String>,
    #[serde(default)]
    pub realname: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub autojoin: Vec<String>,
    #[serde(default = "default_flood_delay")]
    pub flood_delay_ms: u64,
    #[serde(default = "default_reconnect_attempts")]
    pub reconnect_attempts: u32,
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_ms: u64,
    #[serde(default)]
    pub autoconnect: bool,
    #[serde(default)]
    pub bouncer: BouncerType,
    /// Enable bouncer playback on connect (ZNC).
    #[serde(default = "default_true")]
    pub playback: bool,
}

impl NetworkEntry {
    /// Create a new network entry with sensible defaults.
    pub fn new(name: String, address: String, port: u16) -> Self {
        let tls = port == 6697;
        NetworkEntry {
            name,
            address,
            port,
            tls,
            auth_method: AuthMethod::None,
            sasl_mechanism: SaslMechanism::Plain,
            sasl_username: String::new(),
            sasl_password: None,
            nickserv_password: None,
            nick: None,
            realname: None,
            username: None,
            autojoin: Vec::new(),
            flood_delay_ms: default_flood_delay(),
            reconnect_attempts: default_reconnect_attempts(),
            reconnect_delay_ms: default_reconnect_delay(),
            autoconnect: false,
            bouncer: BouncerType::None,
            playback: true,
        }
    }

    /// Set a field by key name. Returns Ok(()) on success, Err with message on unknown key.
    pub fn set_field(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "address" => self.address = value.to_string(),
            "port" => self.port = value.parse().map_err(|_| "invalid port number")?,
            "tls" => self.tls = value.parse().map_err(|_| "expected true or false")?,
            "auth_method" => {
                self.auth_method = match value {
                    "sasl" => AuthMethod::Sasl,
                    "nickserv" => AuthMethod::Nickserv,
                    "none" => AuthMethod::None,
                    _ => return Err("expected sasl, nickserv, or none".to_string()),
                }
            }
            "sasl_mechanism" => {
                self.sasl_mechanism = match value.to_uppercase().as_str() {
                    "PLAIN" => SaslMechanism::Plain,
                    "SCRAM-SHA-256" => SaslMechanism::ScramSha256,
                    "EXTERNAL" => SaslMechanism::External,
                    _ => return Err("expected PLAIN, SCRAM-SHA-256, or EXTERNAL".to_string()),
                }
            }
            "sasl_username" => self.sasl_username = value.to_string(),
            "sasl_password" => self.sasl_password = Some(value.to_string()),
            "nickserv_password" => self.nickserv_password = Some(value.to_string()),
            "nick" => self.nick = Some(value.to_string()),
            "realname" => self.realname = Some(value.to_string()),
            "username" => self.username = Some(value.to_string()),
            "autojoin" => {
                self.autojoin = value.split(',').map(|s| s.trim().to_string()).collect();
            }
            "flood_delay_ms" => self.flood_delay_ms = value.parse().map_err(|_| "invalid number")?,
            "reconnect_attempts" => self.reconnect_attempts = value.parse().map_err(|_| "invalid number")?,
            "reconnect_delay_ms" => self.reconnect_delay_ms = value.parse().map_err(|_| "invalid number")?,
            "autoconnect" => self.autoconnect = value.parse().map_err(|_| "expected true or false")?,
            "bouncer" => {
                self.bouncer = match value {
                    "znc" => BouncerType::Znc,
                    "soju" => BouncerType::Soju,
                    "none" => BouncerType::None,
                    _ => return Err("expected znc, soju, or none".to_string()),
                }
            }
            "playback" => self.playback = value.parse().map_err(|_| "expected true or false")?,
            _ => return Err(format!("unknown field: {}", key)),
        }
        Ok(())
    }
}

impl From<NetworkEntry> for ServerConfig {
    fn from(entry: NetworkEntry) -> Self {
        ServerConfig {
            server: ServerConnectionConfig {
                name: entry.name,
                address: entry.address,
                port: entry.port,
                tls: entry.tls,
                password: None,
            },
            auth: AuthConfig {
                method: entry.auth_method,
                sasl_mechanism: entry.sasl_mechanism,
                sasl_username: entry.sasl_username,
                sasl_password: entry.sasl_password,
                client_cert: None,
                nickserv_password: entry.nickserv_password,
            },
            identity: IdentityConfig {
                nick: entry.nick,
                alt_nicks: Vec::new(),
                realname: entry.realname,
                username: entry.username,
            },
            channels: ChannelConfig {
                autojoin: entry.autojoin,
                keys: HashMap::new(),
            },
            advanced: AdvancedConfig {
                encoding: default_encoding(),
                flood_delay_ms: entry.flood_delay_ms,
                reconnect_attempts: entry.reconnect_attempts,
                reconnect_delay_ms: entry.reconnect_delay_ms,
            },
        }
    }
}

impl From<&ServerConfig> for NetworkEntry {
    fn from(config: &ServerConfig) -> Self {
        NetworkEntry {
            name: config.server.name.clone(),
            address: config.server.address.clone(),
            port: config.server.port,
            tls: config.server.tls,
            auth_method: config.auth.method.clone(),
            sasl_mechanism: config.auth.sasl_mechanism.clone(),
            sasl_username: config.auth.sasl_username.clone(),
            sasl_password: config.auth.sasl_password.clone(),
            nickserv_password: config.auth.nickserv_password.clone(),
            nick: config.identity.nick.clone(),
            realname: config.identity.realname.clone(),
            username: config.identity.username.clone(),
            autojoin: config.channels.autojoin.clone(),
            flood_delay_ms: config.advanced.flood_delay_ms,
            reconnect_attempts: config.advanced.reconnect_attempts,
            reconnect_delay_ms: config.advanced.reconnect_delay_ms,
            autoconnect: false,
            bouncer: BouncerType::None,
            playback: true,
        }
    }
}

impl IrcConfig {
    /// Find a network by name.
    pub fn find(&self, name: &str) -> Option<&NetworkEntry> {
        self.networks.iter().find(|n| n.name == name)
    }

    /// Find a network by name (mutable).
    pub fn find_mut(&mut self, name: &str) -> Option<&mut NetworkEntry> {
        self.networks.iter_mut().find(|n| n.name == name)
    }

    /// Remove a network by name. Returns true if found.
    pub fn remove(&mut self, name: &str) -> bool {
        let len = self.networks.len();
        self.networks.retain(|n| n.name != name);
        self.networks.len() < len
    }

    /// Add a network. Returns Err if name already exists.
    pub fn add(&mut self, entry: NetworkEntry) -> Result<(), String> {
        if self.find(&entry.name).is_some() {
            return Err(format!("network '{}' already exists", entry.name));
        }
        self.networks.push(entry);
        Ok(())
    }

    /// List network names.
    pub fn names(&self) -> Vec<&str> {
        self.networks.iter().map(|n| n.name.as_str()).collect()
    }
}
