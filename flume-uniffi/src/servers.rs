//! Server/network surface. M3 covers config persistence and the connection
//! lifecycle. Events are tracked internally to keep `list_servers()`
//! accurate, but no forwarding to the EventCallback yet — that's M4.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use flume_core::config::general::{CtcpConfig, GeneralConfig};
use flume_core::config::server::{
    self as core_server, IrcConfig, NetworkEntry, ServerConfig,
};
use flume_core::connection::ServerConnection;
use flume_core::event::{ConnectionState as CoreConnectionState, IrcEvent, UserCommand};

use crate::{error::FlumeError, paths, FlumeClient};

// ───────────────────────── public FFI types ─────────────────────────

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
    /// Plaintext or a `${vault_key}` reference resolved at connect time.
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

// ───────────────────────── internal state ─────────────────────────

#[derive(Clone)]
pub(crate) struct ServerSnapshot {
    pub state: ConnectionState,
    pub our_nick: Option<String>,
    pub user_modes: String,
    pub capabilities: Vec<String>,
}

pub(crate) struct ServerHandle {
    pub snapshot: Arc<Mutex<ServerSnapshot>>,
    /// Used in M5 (commands) to push UserCommand::SendMessage / Join / etc.
    /// Kept here so the per-server tx is reachable from the FFI command surface.
    #[allow(dead_code)]
    pub command_tx: mpsc::Sender<UserCommand>,
    pub shutdown_tx: mpsc::Sender<()>,
    pub task: JoinHandle<()>,
}

// ───────────────────────── conversions ─────────────────────────

fn auth_method_to_core(m: &AuthMethod) -> core_server::AuthMethod {
    match m {
        AuthMethod::None => core_server::AuthMethod::None,
        AuthMethod::Sasl => core_server::AuthMethod::Sasl,
        AuthMethod::NickServ => core_server::AuthMethod::Nickserv,
    }
}

fn auth_method_from_core(m: &core_server::AuthMethod) -> AuthMethod {
    match m {
        core_server::AuthMethod::None => AuthMethod::None,
        core_server::AuthMethod::Sasl => AuthMethod::Sasl,
        core_server::AuthMethod::Nickserv => AuthMethod::NickServ,
    }
}

fn bouncer_to_core(b: &BouncerType) -> core_server::BouncerType {
    match b {
        BouncerType::None => core_server::BouncerType::None,
        BouncerType::Znc => core_server::BouncerType::Znc,
        BouncerType::Soju => core_server::BouncerType::Soju,
    }
}

fn bouncer_from_core(b: &core_server::BouncerType) -> BouncerType {
    match b {
        core_server::BouncerType::None => BouncerType::None,
        core_server::BouncerType::Znc => BouncerType::Znc,
        core_server::BouncerType::Soju => BouncerType::Soju,
    }
}

pub(crate) fn network_config_to_entry(c: &NetworkConfig) -> NetworkEntry {
    // Route the single `auth.password` to whichever core field matches the
    // method. Splitting SASL vs NickServ passwords in the FFI surface can be
    // revisited if a user actually needs both on the same network.
    let mut entry = NetworkEntry::new(c.name.clone(), c.address.clone(), c.port);
    entry.tls = c.tls;
    entry.tls_accept_invalid_certs = c.tls_accept_invalid_certs;
    entry.nick = c.nick.clone();
    entry.username = c.username.clone();
    entry.realname = c.realname.clone();
    entry.autojoin = c.autojoin.clone();
    entry.auth_method = auth_method_to_core(&c.auth.method);
    entry.sasl_username = c.auth.username.clone().unwrap_or_default();
    match c.auth.method {
        AuthMethod::Sasl => entry.sasl_password = c.auth.password.clone(),
        AuthMethod::NickServ => entry.nickserv_password = c.auth.password.clone(),
        AuthMethod::None => {}
    }
    entry.autoconnect = c.autoconnect;
    entry.bouncer = bouncer_to_core(&c.bouncer);
    entry.password = c.password.clone();
    entry
}

pub(crate) fn network_config_from_entry(e: &NetworkEntry) -> NetworkConfig {
    let (auth_password, auth_username) = match e.auth_method {
        core_server::AuthMethod::Sasl => (
            e.sasl_password.clone(),
            if e.sasl_username.is_empty() {
                None
            } else {
                Some(e.sasl_username.clone())
            },
        ),
        core_server::AuthMethod::Nickserv => (e.nickserv_password.clone(), None),
        core_server::AuthMethod::None => (None, None),
    };
    NetworkConfig {
        name: e.name.clone(),
        address: e.address.clone(),
        port: e.port,
        tls: e.tls,
        tls_accept_invalid_certs: e.tls_accept_invalid_certs,
        nick: e.nick.clone(),
        username: e.username.clone(),
        realname: e.realname.clone(),
        autojoin: e.autojoin.clone(),
        auth: AuthConfig {
            method: auth_method_from_core(&e.auth_method),
            username: auth_username,
            password: auth_password,
        },
        autoconnect: e.autoconnect,
        bouncer: bouncer_from_core(&e.bouncer),
        password: e.password.clone(),
    }
}

fn core_state_to_ffi(s: &CoreConnectionState) -> ConnectionState {
    match s {
        CoreConnectionState::Disconnected => ConnectionState::Disconnected,
        CoreConnectionState::Connecting => ConnectionState::Connecting,
        CoreConnectionState::Registering => ConnectionState::Registering,
        CoreConnectionState::Connected => ConnectionState::Connected,
    }
}

// ───────────────────────── persistence ─────────────────────────

pub(crate) fn load_irc_config_from(path: &std::path::Path) -> Result<IrcConfig, FlumeError> {
    if !path.exists() {
        return Ok(IrcConfig::default());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|e| FlumeError::Config(format!("read {}: {}", path.display(), e)))?;
    toml::from_str(&contents)
        .map_err(|e| FlumeError::Config(format!("parse {}: {}", path.display(), e)))
}

fn save_irc_config_to(path: &std::path::Path, config: &IrcConfig) -> Result<(), FlumeError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| FlumeError::Config(format!("mkdir {}: {}", parent.display(), e)))?;
    }
    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| FlumeError::Config(format!("serialize: {}", e)))?;
    std::fs::write(path, toml_str)
        .map_err(|e| FlumeError::Config(format!("write {}: {}", path.display(), e)))?;
    Ok(())
}

// ───────────────────────── exported impl ─────────────────────────

#[uniffi::export]
impl FlumeClient {
    /// Add or replace a network in the in-memory list. Call `save_networks()`
    /// to persist. Validates name is non-empty.
    pub fn add_network(&self, config: NetworkConfig) -> Result<(), FlumeError> {
        if config.name.is_empty() {
            return Err(FlumeError::InvalidArgument("network name is empty".into()));
        }
        let entry = network_config_to_entry(&config);
        let mut state = self.state.lock().expect("state poisoned");
        // Replace if exists; add otherwise.
        if let Some(slot) = state.irc_config.find_mut(&config.name) {
            *slot = entry;
        } else {
            state
                .irc_config
                .add(entry)
                .map_err(|e| FlumeError::Network(e))?;
        }
        Ok(())
    }

    pub fn remove_network(&self, name: String) {
        self.state
            .lock()
            .expect("state poisoned")
            .irc_config
            .remove(&name);
    }

    pub fn list_networks(&self) -> Vec<NetworkConfig> {
        self.state
            .lock()
            .expect("state poisoned")
            .irc_config
            .networks
            .iter()
            .map(network_config_from_entry)
            .collect()
    }

    /// Flushes the in-memory network list to
    /// `~/Library/Application Support/FlumeMac/irc.toml`. Mirrors the TUI's
    /// explicit `/save` convention — mutations are not auto-persisted.
    pub fn save_networks(&self) -> Result<(), FlumeError> {
        let path = paths::irc_config_path().ok_or_else(|| {
            FlumeError::Config("could not resolve irc.toml path ($HOME unset?)".into())
        })?;
        let snapshot = self
            .state
            .lock()
            .expect("state poisoned")
            .irc_config
            .clone();
        save_irc_config_to(&path, &snapshot)
    }

    /// Spawn a connection task for the named network. Idempotent — repeat
    /// calls for an already-connected name are a no-op. Returns once the
    /// task is spawned, not once IRC registration completes.
    pub fn connect_network(&self, name: String) -> Result<(), FlumeError> {
        let mut state = self.state.lock().expect("state poisoned");
        if state.servers.contains_key(&name) {
            return Ok(());
        }
        let entry = state
            .irc_config
            .find(&name)
            .ok_or_else(|| FlumeError::Network(format!("no network named '{}'", name)))?
            .clone();

        let server_config: ServerConfig = entry.clone().into();
        let vault = state.vault.clone();

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let (conn, mut handle) = ServerConnection::new(
            server_config,
            GeneralConfig::default(),
            vault,
            CtcpConfig::default(),
        );
        let command_tx = handle.command_tx.clone();

        let snapshot = Arc::new(Mutex::new(ServerSnapshot {
            state: ConnectionState::Connecting,
            our_nick: None,
            user_modes: String::new(),
            capabilities: Vec::new(),
        }));

        let snapshot_task = Arc::clone(&snapshot);
        let autojoin = entry.autojoin.clone();
        let cmd_tx_task = command_tx.clone();
        let task = self.runtime.spawn(async move {
            let conn_task = tokio::spawn(conn.run());

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.recv() => {
                        let _ = cmd_tx_task
                            .send(UserCommand::Quit(Some("FlumeMac".to_string())))
                            .await;
                        let drain = async {
                            while let Some(ev) = handle.event_rx.recv().await {
                                if matches!(ev, IrcEvent::Disconnected { .. }) {
                                    break;
                                }
                            }
                        };
                        let _ =
                            tokio::time::timeout(Duration::from_millis(500), drain).await;
                        conn_task.abort();
                        break;
                    }
                    maybe_ev = handle.event_rx.recv() => {
                        match maybe_ev {
                            Some(ev) => {
                                update_snapshot(&snapshot_task, &ev);
                                // Send autojoin on first Connected. M4 will
                                // also forward the event to the EventCallback.
                                if let IrcEvent::Connected { .. } = &ev {
                                    for chan in &autojoin {
                                        let _ = cmd_tx_task
                                            .send(UserCommand::Join {
                                                channel: chan.clone(),
                                                key: None,
                                            })
                                            .await;
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        state.servers.insert(
            name,
            ServerHandle {
                snapshot,
                command_tx,
                shutdown_tx,
                task,
            },
        );
        Ok(())
    }

    /// Send QUIT and wait briefly for the connection to flush, then abort
    /// the task. Blocks the calling Swift thread for up to ~1 second.
    pub fn disconnect_network(&self, name: String, _reason: Option<String>) {
        // `_reason` is accepted but not yet plumbed into UserCommand::Quit —
        // M4 will surface it once we revisit the QUIT path with the
        // event-forwarding logic.
        let Some(handle) = self
            .state
            .lock()
            .expect("state poisoned")
            .servers
            .remove(&name)
        else {
            return;
        };
        let _ = handle.shutdown_tx.try_send(());
        let _ = self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), handle.task).await
        });
    }

    pub fn list_servers(&self) -> Vec<ServerInfo> {
        let state = self.state.lock().expect("state poisoned");
        state
            .servers
            .iter()
            .map(|(name, handle)| {
                let snap = handle.snapshot.lock().expect("snapshot poisoned");
                ServerInfo {
                    name: name.clone(),
                    state: snap.state.clone(),
                    our_nick: snap.our_nick.clone(),
                    user_modes: snap.user_modes.clone(),
                    capabilities: snap.capabilities.clone(),
                }
            })
            .collect()
    }
}

fn update_snapshot(snapshot: &Mutex<ServerSnapshot>, ev: &IrcEvent) {
    let mut s = snapshot.lock().expect("snapshot poisoned");
    match ev {
        IrcEvent::Connected {
            our_nick,
            capabilities,
            ..
        } => {
            s.state = ConnectionState::Connected;
            s.our_nick = Some(our_nick.clone());
            s.capabilities = capabilities.iter().cloned().collect();
        }
        IrcEvent::StateChanged { state, .. } => {
            s.state = core_state_to_ffi(state);
        }
        IrcEvent::Disconnected { .. } => {
            s.state = ConnectionState::Disconnected;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn with_tmp_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::env::set_var("FLUME_MAC_CONFIG_DIR", tmp.path());
        tmp
    }

    fn sample_network(name: &str) -> NetworkConfig {
        NetworkConfig {
            name: name.into(),
            address: "irc.example.com".into(),
            port: 6697,
            tls: true,
            tls_accept_invalid_certs: false,
            nick: Some("alice".into()),
            username: Some("alice".into()),
            realname: Some("Alice".into()),
            autojoin: vec!["#chan".into()],
            auth: AuthConfig {
                method: AuthMethod::Sasl,
                username: Some("alice".into()),
                password: Some("${sasl_pw}".into()),
            },
            autoconnect: false,
            bouncer: BouncerType::None,
            password: None,
        }
    }

    #[test]
    #[serial]
    fn persist_roundtrip() {
        let _tmp = with_tmp_dir();
        let client = FlumeClient::new();

        client.add_network(sample_network("libera")).unwrap();
        client.add_network(sample_network("oftc")).unwrap();
        assert_eq!(client.list_networks().len(), 2);
        client.save_networks().unwrap();

        // Fresh client reads the persisted file
        let client2 = FlumeClient::new();
        let nets = client2.list_networks();
        assert_eq!(nets.len(), 2);
        let names: Vec<&str> = nets.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"libera"));
        assert!(names.contains(&"oftc"));

        // Roundtrip preserved auth method + password routing
        let n = nets.iter().find(|n| n.name == "libera").unwrap();
        assert!(matches!(n.auth.method, AuthMethod::Sasl));
        assert_eq!(n.auth.password.as_deref(), Some("${sasl_pw}"));
        assert_eq!(n.autojoin, vec!["#chan".to_string()]);
    }

    #[test]
    #[serial]
    fn add_replaces_existing() {
        let _tmp = with_tmp_dir();
        let client = FlumeClient::new();

        client.add_network(sample_network("libera")).unwrap();
        let mut updated = sample_network("libera");
        updated.address = "irc2.example.com".into();
        client.add_network(updated).unwrap();
        assert_eq!(client.list_networks().len(), 1);
        assert_eq!(client.list_networks()[0].address, "irc2.example.com");
    }

    #[test]
    #[serial]
    fn remove_network_works() {
        let _tmp = with_tmp_dir();
        let client = FlumeClient::new();

        client.add_network(sample_network("libera")).unwrap();
        client.remove_network("libera".into());
        assert!(client.list_networks().is_empty());
    }

    #[test]
    #[serial]
    fn add_rejects_empty_name() {
        let _tmp = with_tmp_dir();
        let client = FlumeClient::new();
        let mut bad = sample_network("");
        bad.name = String::new();
        assert!(matches!(
            client.add_network(bad),
            Err(FlumeError::InvalidArgument(_)),
        ));
    }

    /// Live IRC test — requires network access to Libera. Run with:
    ///   cargo test -p flume-uniffi -- --ignored connect_libera_live
    #[test]
    #[ignore]
    #[serial]
    fn connect_libera_live() {
        let _tmp = with_tmp_dir();
        let client = FlumeClient::new();

        let mut cfg = sample_network("libera");
        cfg.address = "irc.libera.chat".into();
        cfg.nick = Some(format!("flmtest{}", std::process::id() % 1000));
        cfg.auth.method = AuthMethod::None;
        cfg.auth.password = None;
        cfg.autojoin = vec![]; // don't join anything
        client.add_network(cfg).unwrap();

        client.connect_network("libera".into()).unwrap();

        // Wait up to 15s for Connected.
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let servers = client.list_servers();
            let s = servers.first().expect("server tracked");
            if matches!(s.state, ConnectionState::Connected) {
                assert!(s.our_nick.is_some());
                assert!(!s.capabilities.is_empty(), "expected IRCv3 caps");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for Connected (state={:?})",
                s.state,
            );
            std::thread::sleep(Duration::from_millis(200));
        }

        client.disconnect_network("libera".into(), None);
        assert!(client.list_servers().is_empty());
    }
}
