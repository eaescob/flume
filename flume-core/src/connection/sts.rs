use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// An STS (Strict Transport Security) policy for an IRC server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StsPolicy {
    pub host: String,
    pub port: u16,
    pub duration: u64,
    pub preload: bool,
    /// When this policy was last updated (Unix timestamp).
    pub updated_at: i64,
}

impl StsPolicy {
    /// Check if this policy is still valid.
    pub fn is_valid(&self) -> bool {
        if self.duration == 0 {
            return false;
        }
        let now = chrono::Utc::now().timestamp();
        now - self.updated_at < self.duration as i64
    }
}

/// Cache of STS policies, persisted to disk.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StsPolicyCache {
    #[serde(default)]
    pub policies: HashMap<String, StsPolicy>,
}

impl StsPolicyCache {
    /// Load the policy cache from disk.
    pub fn load() -> Self {
        let path = cache_path();
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        toml::from_str(&contents).unwrap_or_default()
    }

    /// Save the policy cache to disk.
    pub fn save(&self) {
        let path = cache_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(toml_str) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&path, toml_str);
        }
    }

    /// Check if a host has a valid STS policy. Returns the TLS port if so.
    pub fn check(&self, host: &str) -> Option<u16> {
        self.policies
            .get(host)
            .filter(|p| p.is_valid())
            .map(|p| p.port)
    }

    /// Store or update an STS policy.
    pub fn update(&mut self, host: &str, port: u16, duration: u64, preload: bool) {
        let policy = StsPolicy {
            host: host.to_string(),
            port,
            duration,
            preload,
            updated_at: chrono::Utc::now().timestamp(),
        };
        self.policies.insert(host.to_string(), policy);
        self.save();
    }

    /// Remove expired policies.
    pub fn prune(&mut self) {
        self.policies.retain(|_, p| p.is_valid());
        self.save();
    }
}

/// Parse STS value from CAP LS response.
/// Format: `sts=port=6697,duration=2592000` or `sts=port=6697,duration=2592000,preload`
pub fn parse_sts_value(value: &str) -> Option<(u16, u64, bool)> {
    let mut port = None;
    let mut duration = None;
    let mut preload = false;

    for part in value.split(',') {
        let part = part.trim();
        if let Some(p) = part.strip_prefix("port=") {
            port = p.parse().ok();
        } else if let Some(d) = part.strip_prefix("duration=") {
            duration = d.parse().ok();
        } else if part == "preload" {
            preload = true;
        }
    }

    Some((port?, duration?, preload))
}

fn cache_path() -> PathBuf {
    crate::config::data_dir().join("sts_policies.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sts_basic() {
        let (port, duration, preload) = parse_sts_value("port=6697,duration=2592000").unwrap();
        assert_eq!(port, 6697);
        assert_eq!(duration, 2592000);
        assert!(!preload);
    }

    #[test]
    fn parse_sts_with_preload() {
        let (port, duration, preload) =
            parse_sts_value("port=6697,duration=2592000,preload").unwrap();
        assert_eq!(port, 6697);
        assert!(preload);
        assert_eq!(duration, 2592000);
    }

    #[test]
    fn parse_sts_invalid() {
        assert!(parse_sts_value("garbage").is_none());
        assert!(parse_sts_value("port=abc").is_none());
    }

    #[test]
    fn policy_validity() {
        let mut policy = StsPolicy {
            host: "irc.libera.chat".to_string(),
            port: 6697,
            duration: 3600,
            preload: false,
            updated_at: chrono::Utc::now().timestamp(),
        };
        assert!(policy.is_valid());

        policy.duration = 0;
        assert!(!policy.is_valid());

        policy.duration = 3600;
        policy.updated_at = chrono::Utc::now().timestamp() - 7200; // 2 hours ago
        assert!(!policy.is_valid());
    }

    #[test]
    fn cache_check() {
        let mut cache = StsPolicyCache::default();
        assert!(cache.check("irc.libera.chat").is_none());

        cache.policies.insert(
            "irc.libera.chat".to_string(),
            StsPolicy {
                host: "irc.libera.chat".to_string(),
                port: 6697,
                duration: 86400,
                preload: false,
                updated_at: chrono::Utc::now().timestamp(),
            },
        );

        assert_eq!(cache.check("irc.libera.chat"), Some(6697));
        assert!(cache.check("other.server").is_none());
    }
}
