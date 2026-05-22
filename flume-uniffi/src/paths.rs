//! Mac-idiomatic config and data paths. Deliberately *not* shared with the
//! Flume TUI — see project_flumemac memory: "no shared config files".
//!
//! Path helpers are consumed by upcoming milestones (networks in M3,
//! snotice in M6); allow dead_code until then.
#![allow(dead_code)]

use std::path::PathBuf;

const APP_NAME: &str = "FlumeMac";

/// Override env var used by tests to redirect all paths to a tmp dir.
/// When set, `config_dir`, `logs_dir`, and all derived paths resolve under it.
const CONFIG_OVERRIDE_ENV: &str = "FLUME_MAC_CONFIG_DIR";

fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// `~/Library/Application Support/FlumeMac/` — networks, snotice rules, vault.
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(CONFIG_OVERRIDE_ENV) {
        return Some(PathBuf::from(path));
    }
    home().map(|h| h.join("Library/Application Support").join(APP_NAME))
}

/// `~/Library/Logs/FlumeMac/` — flume-uniffi tracing output. When the test
/// override is active, logs go alongside the override config dir.
pub fn logs_dir() -> Option<PathBuf> {
    if std::env::var(CONFIG_OVERRIDE_ENV).is_ok() {
        return config_dir().map(|d| d.join("logs"));
    }
    home().map(|h| h.join("Library/Logs").join(APP_NAME))
}

pub fn irc_config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("irc.toml"))
}

pub fn snotice_config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("snotice.toml"))
}

pub fn vault_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("vault.toml"))
}

pub fn log_file() -> Option<PathBuf> {
    logs_dir().map(|d| d.join("flume-uniffi.log"))
}
