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
mod error;
mod events;
mod paths;
mod servers;
mod snotice;
mod vault;

pub use buffers::{BufferInfo, BufferKind, BufferRef, NickEntry};
pub use error::FlumeError;
pub use events::{ChatMessage, Event, EventCallback};
pub use servers::{
    AuthConfig, AuthMethod, BouncerType, ConnectionState, NetworkConfig, ServerInfo,
};
pub use snotice::SnoticeRule;
pub use vault::VaultStatus;

use std::sync::{Mutex, Once};

#[derive(uniffi::Object)]
pub struct FlumeClient {
    runtime: tokio::runtime::Runtime,
    callback: Mutex<Option<Box<dyn EventCallback>>>,
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
        std::sync::Arc::new(Self {
            runtime,
            callback: Mutex::new(None),
        })
    }

    /// Register the callback that receives all events from all connected
    /// servers. Replaces any previous callback. Called once at app startup.
    pub fn set_event_callback(&self, callback: Box<dyn EventCallback>) {
        *self.callback.lock().expect("callback mutex poisoned") = Some(callback);
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

/// Suppress unused-import warnings while the rest of the surface is still
/// stubs. Future milestones consume `runtime` and `callback` directly.
#[allow(dead_code)]
fn _internal_use(client: &FlumeClient) {
    let _ = &client.runtime;
    let _ = &client.callback;
}
