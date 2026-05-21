//! FlumeMac UniFFI bridge to flume-core.
//!
//! Surface scoped to the v1 product: servers, buffers, events, commands,
//! snotice, vault. DCC, scripting, and themes are deferred to v2.
//!
//! Mac-idiomatic config/data paths under `~/Library/Application Support/FlumeMac/`
//! and `~/Library/Logs/FlumeMac/`. No file sharing with the Flume TUI by design.

uniffi::setup_scaffolding!();

mod buffers;
mod commands;
mod dispatch;
mod error;
mod events;
mod paths;
mod servers;
mod snotice;
mod vault;

pub use buffers::{BufferInfo, BufferKind, BufferRef, NickEntry};
pub use error::FlumeError;
pub use events::{Event, EventCallback};
pub use servers::{
    AuthConfig, AuthMethod, BouncerType, ConnectionState, NetworkConfig, ServerInfo,
};
pub use snotice::SnoticeRule;
pub use vault::VaultStatus;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};

use flume_core::config::server::IrcConfig;
use flume_core::config::vault::Vault;

use crate::dispatch::CompiledSnoticeRule;
use crate::servers::ServerHandle;

/// Aggregated mutable state. One mutex covers all of it — contention is
/// negligible because Swift callers operate sequentially per session.
pub(crate) struct ClientState {
    /// Wrapped as Arc so connection tasks can hold their own handle.
    /// `set_event_callback` swaps the Arc — new tasks pick up the new
    /// callback; in-flight tasks continue to use whatever they cloned.
    pub(crate) callback: Option<Arc<dyn EventCallback>>,
    pub(crate) vault: Option<Vault>,
    pub(crate) irc_config: IrcConfig,
    pub(crate) servers: HashMap<String, ServerHandle>,
    /// Compiled snotice rules consulted by the dispatcher. Populated by M6
    /// (add/remove/save endpoints); empty for now.
    pub(crate) snotice_rules: Arc<Mutex<Vec<CompiledSnoticeRule>>>,
}

#[derive(uniffi::Object)]
pub struct FlumeClient {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) state: Mutex<ClientState>,
}

#[uniffi::export]
impl FlumeClient {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        init_tracing();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("flume-uniffi")
            .build()
            .expect("failed to build tokio runtime");

        // Load irc.toml on startup so list_networks() reflects disk state
        // before the Swift app calls anything else. Vault loads lazily on
        // first unlock; servers HashMap starts empty.
        let irc_config = paths::irc_config_path()
            .and_then(|p| servers::load_irc_config_from(&p).ok())
            .unwrap_or_default();

        std::sync::Arc::new(Self {
            runtime,
            state: Mutex::new(ClientState {
                callback: None,
                vault: None,
                irc_config,
                servers: HashMap::new(),
                snotice_rules: Arc::new(Mutex::new(Vec::new())),
            }),
        })
    }

    /// Register the callback that receives all events from all connected
    /// servers. Replaces any previous callback. Called once at app startup.
    pub fn set_event_callback(&self, callback: Box<dyn EventCallback>) {
        // Convert Box → Arc so connection tasks can clone their own handle.
        let arc: Arc<dyn EventCallback> = Arc::from(callback);
        self.state.lock().expect("state poisoned").callback = Some(arc);
    }
}

fn init_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(log_path) = paths::log_file() else { return };
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(file) = std::fs::File::create(&log_path) else { return };
        let _ = tracing_subscriber::fmt()
            .with_writer(file)
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_env("FLUME_LOG").unwrap_or_else(|_| {
                    tracing_subscriber::EnvFilter::new("flume_core=info,flume_uniffi=info")
                }),
            )
            .with_ansi(false)
            .try_init();
    });
}

