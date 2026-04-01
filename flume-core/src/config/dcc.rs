use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DccConfig {
    /// DCC is disabled by default — must be explicitly enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Auto-accept incoming DCC offers (risky, default false).
    #[serde(default)]
    pub auto_accept: bool,
    /// Directory for downloaded files.
    #[serde(default = "default_download_dir")]
    pub download_directory: String,
    /// Port range for listening (DCC SEND outgoing, passive DCC).
    #[serde(default = "default_port_range")]
    pub port_range: (u16, u16),
    /// Prefer passive DCC (NAT-friendly).
    #[serde(default = "default_true")]
    pub passive: bool,
    /// Max file size in bytes (0 = unlimited).
    #[serde(default)]
    pub max_transfer_size: u64,
}

impl Default for DccConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_accept: false,
            download_directory: default_download_dir(),
            port_range: default_port_range(),
            passive: true,
            max_transfer_size: 0,
        }
    }
}

fn default_download_dir() -> String {
    "~/Downloads/flume".to_string()
}

fn default_port_range() -> (u16, u16) {
    (1024, 65535)
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = DccConfig::default();
        assert!(!config.enabled);
        assert!(!config.auto_accept);
        assert!(config.passive);
        assert_eq!(config.port_range, (1024, 65535));
    }

    #[test]
    fn deserialize_dcc_config() {
        let toml_str = r#"
enabled = true
auto_accept = false
download_directory = "/tmp/dcc"
port_range = [4000, 5000]
passive = false
"#;
        let config: DccConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert!(!config.passive);
        assert_eq!(config.port_range, (4000, 5000));
        assert_eq!(config.download_directory, "/tmp/dcc");
    }

    #[test]
    fn deserialize_empty_defaults() {
        let config: DccConfig = toml::from_str("").unwrap();
        assert!(!config.enabled);
        assert!(config.passive);
    }
}
