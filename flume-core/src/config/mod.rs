pub mod dcc;
pub mod general;
pub mod keybindings;
pub mod llm;
pub mod server;
pub mod theme;
pub mod vault;

use std::path::PathBuf;

pub use general::FlumeConfig;
pub use server::{IrcConfig, NetworkEntry, ServerConfig};
pub use vault::Vault;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("config directory not found")]
    NoDirFound,
    #[error("server config not found: {0}")]
    ServerNotFound(String),
}

/// Return the XDG config directory for flume.
/// Falls back to `~/.config/flume/`.
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "flume")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            let mut p = dirs_home().unwrap_or_else(|| PathBuf::from("."));
            p.push(".config/flume");
            p
        })
}

/// Return the XDG data directory for flume.
/// Falls back to `~/.local/share/flume/`.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "flume")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let mut p = dirs_home().unwrap_or_else(|| PathBuf::from("."));
            p.push(".local/share/flume");
            p
        })
}

/// Return the path to the vault file.
pub fn vault_path() -> PathBuf {
    data_dir().join("vault.toml")
}

/// Return the themes directory.
pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

/// Return the path to the irc.toml file (network/server definitions).
pub fn irc_config_path() -> PathBuf {
    config_dir().join("irc.toml")
}

fn dirs_home() -> Option<PathBuf> {
    directories::UserDirs::new().map(|d| d.home_dir().to_path_buf())
}

/// Load the main config from the XDG config path.
/// Returns default config if the file doesn't exist.
pub fn load_config() -> Result<FlumeConfig, ConfigError> {
    let path = config_dir().join("config.toml");
    if !path.exists() {
        return Ok(FlumeConfig::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    let config: FlumeConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// Load the irc.toml network configuration.
/// Returns empty config if the file doesn't exist.
pub fn load_irc_config() -> Result<IrcConfig, ConfigError> {
    let path = irc_config_path();
    if !path.exists() {
        return Ok(IrcConfig::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    let config: IrcConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// Save the irc.toml network configuration.
pub fn save_irc_config(config: &IrcConfig) -> Result<(), ConfigError> {
    let path = irc_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| ConfigError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    std::fs::write(&path, toml_str)?;
    Ok(())
}

/// Find a network in irc.toml by name and convert to ServerConfig.
pub fn find_network(config: &IrcConfig, name: &str) -> Option<ServerConfig> {
    config.find(name).cloned().map(ServerConfig::from)
}

/// Load a server config by name.
/// Checks irc.toml first, then falls back to individual server files.
pub fn load_server_config(name: &str) -> Result<ServerConfig, ConfigError> {
    // Try irc.toml first
    let irc_config = load_irc_config()?;
    if let Some(server_config) = find_network(&irc_config, name) {
        return Ok(server_config);
    }

    // Fall back to individual file
    let path = config_dir().join("servers").join(format!("{}.toml", name));
    if !path.exists() {
        return Err(ConfigError::ServerNotFound(name.to_string()));
    }
    let contents = std::fs::read_to_string(&path)?;
    let config: ServerConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// List all available server/network names.
/// Combines names from irc.toml and individual server files.
pub fn list_server_configs() -> Result<Vec<String>, ConfigError> {
    let mut names = Vec::new();

    // From irc.toml
    let irc_config = load_irc_config()?;
    for net in &irc_config.networks {
        names.push(net.name.clone());
    }

    // From individual files (legacy fallback)
    let dir = config_dir().join("servers");
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(stem) = path.file_stem() {
                    let name = stem.to_string_lossy().to_string();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
    }

    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_config() {
        let config = FlumeConfig::default();
        assert_eq!(config.general.default_nick, "flume_user");
        assert_eq!(config.ui.tick_rate_fps, 30);
        assert!(config.logging.enabled);
    }

    #[test]
    fn deserialize_full_config() {
        let toml_str = r##"
[general]
default_nick = "emilio"
alt_nicks = ["emilio_", "emilio__"]
realname = "Emilio"
username = "emilio"
quit_message = "Bye!"
timestamp_format = "%H:%M"
scrollback_lines = 5000

[ui]
theme = "solarized-dark"
layout = "default"
show_server_tree = true
show_nick_list = false
tick_rate_fps = 60

[logging]
enabled = true
format = "json"

[notifications]
highlight_words = ["emilio", "flume"]
notify_private = true

[ctcp]
version_reply = "Flume 0.1.0"
rate_limit = 5
"##;
        let config: FlumeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.default_nick, "emilio");
        assert_eq!(config.general.alt_nicks, vec!["emilio_", "emilio__"]);
        assert!(!config.ui.show_nick_list);
        assert_eq!(config.ui.tick_rate_fps, 60);
        assert_eq!(config.logging.format, "json");
        assert_eq!(config.ctcp.rate_limit, 5);
    }

    #[test]
    fn deserialize_minimal_config() {
        let toml_str = "";
        let config: FlumeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.default_nick, "flume_user");
    }

    #[test]
    fn deserialize_server_config() {
        let toml_str = r##"
[server]
name = "Libera Chat"
address = "irc.libera.chat"
port = 6697
tls = true

[auth]
method = "sasl"
sasl_mechanism = "PLAIN"
sasl_username = "emilio"
sasl_password = "${libera_pass}"

[channels]
autojoin = ["#rust", "#linux"]
"##;
        let config: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.name, "Libera Chat");
        assert_eq!(config.server.port, 6697);
        assert!(config.server.tls);
        assert_eq!(config.auth.method, server::AuthMethod::Sasl);
        assert_eq!(config.auth.sasl_password, Some("${libera_pass}".to_string()));
        assert_eq!(config.channels.autojoin, vec!["#rust", "#linux"]);
    }

    #[test]
    fn deserialize_server_config_plain_tcp() {
        let toml_str = r##"
[server]
name = "Legacy Server"
address = "irc.example.com"
port = 6667
tls = false
"##;
        let config: ServerConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.server.tls);
        assert_eq!(config.server.port, 6667);
        assert_eq!(config.auth.method, server::AuthMethod::None);
    }

    #[test]
    fn config_serialize_round_trip() {
        let config = FlumeConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: FlumeConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.general.default_nick, config.general.default_nick);
        assert_eq!(parsed.ui.tick_rate_fps, config.ui.tick_rate_fps);
    }

    #[test]
    fn deserialize_irc_config() {
        let toml_str = r##"
[[network]]
name = "libera"
address = "irc.libera.chat"
port = 6697
tls = true
auth_method = "sasl"
sasl_mechanism = "PLAIN"
sasl_username = "emilio"
sasl_password = "${libera_pass}"
autojoin = ["#rust", "#flume"]

[[network]]
name = "efnet"
address = "irc.efnet.org"
port = 6667
tls = false
auth_method = "none"
autojoin = ["#test"]
"##;
        let config: IrcConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.networks.len(), 2);
        assert_eq!(config.networks[0].name, "libera");
        assert_eq!(config.networks[0].address, "irc.libera.chat");
        assert!(config.networks[0].tls);
        assert_eq!(config.networks[1].name, "efnet");
        assert!(!config.networks[1].tls);
        assert_eq!(config.networks[1].port, 6667);
    }

    #[test]
    fn deserialize_empty_irc_config() {
        let config: IrcConfig = toml::from_str("").unwrap();
        assert!(config.networks.is_empty());
    }

    #[test]
    fn network_entry_to_server_config() {
        let entry = NetworkEntry::new("test".to_string(), "irc.test.com".to_string(), 6697);
        let sc = ServerConfig::from(entry);
        assert_eq!(sc.server.name, "test");
        assert_eq!(sc.server.address, "irc.test.com");
        assert_eq!(sc.server.port, 6697);
        assert!(sc.server.tls);
        assert_eq!(sc.auth.method, server::AuthMethod::None);
    }

    #[test]
    fn server_config_to_network_entry_round_trip() {
        let toml_str = r##"
[[network]]
name = "libera"
address = "irc.libera.chat"
port = 6697
tls = true
auth_method = "sasl"
sasl_username = "emilio"
sasl_password = "${pass}"
autojoin = ["#rust"]
"##;
        let config: IrcConfig = toml::from_str(toml_str).unwrap();
        let entry = &config.networks[0];
        let sc = ServerConfig::from(entry.clone());
        let back = NetworkEntry::from(&sc);
        assert_eq!(back.name, "libera");
        assert_eq!(back.address, "irc.libera.chat");
        assert_eq!(back.sasl_username, "emilio");
        assert_eq!(back.autojoin, vec!["#rust"]);
    }

    #[test]
    fn irc_config_find_and_remove() {
        let mut config = IrcConfig::default();
        config.add(NetworkEntry::new("a".into(), "a.com".into(), 6697)).unwrap();
        config.add(NetworkEntry::new("b".into(), "b.com".into(), 6667)).unwrap();

        assert!(config.find("a").is_some());
        assert!(config.find("c").is_none());
        assert_eq!(config.names(), vec!["a", "b"]);

        assert!(config.remove("a"));
        assert!(config.find("a").is_none());
        assert!(!config.remove("a"));
    }

    #[test]
    fn irc_config_add_duplicate() {
        let mut config = IrcConfig::default();
        config.add(NetworkEntry::new("a".into(), "a.com".into(), 6697)).unwrap();
        assert!(config.add(NetworkEntry::new("a".into(), "x.com".into(), 6697)).is_err());
    }

    #[test]
    fn network_entry_set_field() {
        let mut entry = NetworkEntry::new("test".into(), "old.com".into(), 6667);
        entry.set_field("address", "new.com").unwrap();
        assert_eq!(entry.address, "new.com");
        entry.set_field("port", "6697").unwrap();
        assert_eq!(entry.port, 6697);
        entry.set_field("tls", "true").unwrap();
        assert!(entry.tls);
        entry.set_field("auth_method", "sasl").unwrap();
        assert_eq!(entry.auth_method, server::AuthMethod::Sasl);
        entry.set_field("autojoin", "#a, #b").unwrap();
        assert_eq!(entry.autojoin, vec!["#a", "#b"]);
        assert!(entry.set_field("bogus", "val").is_err());
    }

    #[test]
    fn irc_config_serialize_round_trip() {
        let mut config = IrcConfig::default();
        config.add(NetworkEntry::new("libera".into(), "irc.libera.chat".into(), 6697)).unwrap();
        config.add(NetworkEntry::new("efnet".into(), "irc.efnet.org".into(), 6667)).unwrap();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: IrcConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.networks.len(), 2);
        assert_eq!(parsed.networks[0].name, "libera");
        assert_eq!(parsed.networks[1].name, "efnet");
    }

    #[test]
    fn find_network_converts() {
        let mut config = IrcConfig::default();
        let mut entry = NetworkEntry::new("test".into(), "irc.test.com".into(), 6697);
        entry.autojoin = vec!["#hello".into()];
        config.add(entry).unwrap();

        let sc = find_network(&config, "test").unwrap();
        assert_eq!(sc.server.address, "irc.test.com");
        assert_eq!(sc.channels.autojoin, vec!["#hello"]);
        assert!(find_network(&config, "missing").is_none());
    }
}
