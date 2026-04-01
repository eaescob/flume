use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Direction of a split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

/// Runtime state for an active split.
#[derive(Debug, Clone)]
pub struct SplitState {
    pub direction: SplitDirection,
    /// Server name for the secondary pane.
    pub secondary_server: String,
    /// Buffer name within that server for the secondary pane.
    pub secondary_buffer: String,
    /// Percentage of space for the primary (left/top) pane (1-99).
    pub ratio: u16,
}

impl SplitState {
    pub fn new(direction: SplitDirection, server: String, buffer: String) -> Self {
        Self {
            direction,
            secondary_server: server,
            secondary_buffer: buffer,
            ratio: 50,
        }
    }
}

/// Serializable layout profile for save/load.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LayoutProfile {
    pub direction: SplitDirection,
    pub primary: String,
    pub secondary: String,
    #[serde(default = "default_ratio")]
    pub ratio: u16,
}

fn default_ratio() -> u16 {
    50
}

/// Return the layouts directory (~/.config/flume/layouts/).
pub fn layouts_dir() -> PathBuf {
    flume_core::config::config_dir().join("layouts")
}

/// Save a layout profile to disk.
pub fn save_layout(name: &str, profile: &LayoutProfile) -> std::io::Result<()> {
    let dir = layouts_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.toml", name));
    let toml_str = toml::to_string_pretty(profile)
        .map_err(std::io::Error::other)?;
    std::fs::write(&path, toml_str)
}

/// Load a layout profile from disk.
pub fn load_layout(name: &str) -> Option<LayoutProfile> {
    let path = layouts_dir().join(format!("{}.toml", name));
    let contents = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&contents).ok()
}

/// List all saved layout names.
pub fn list_layouts() -> Vec<String> {
    let dir = layouts_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// Delete a saved layout. Returns true if the file existed.
pub fn delete_layout(name: &str) -> bool {
    let path = layouts_dir().join(format!("{}.toml", name));
    std::fs::remove_file(&path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("flume_test_layouts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn layout_profile_round_trip() {
        let profile = LayoutProfile {
            direction: SplitDirection::Vertical,
            primary: "#rust".to_string(),
            secondary: "#linux".to_string(),
            ratio: 60,
        };
        let toml_str = toml::to_string_pretty(&profile).unwrap();
        let parsed: LayoutProfile = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.direction, SplitDirection::Vertical);
        assert_eq!(parsed.primary, "#rust");
        assert_eq!(parsed.secondary, "#linux");
        assert_eq!(parsed.ratio, 60);
    }

    #[test]
    fn layout_profile_defaults_ratio() {
        let toml_str = r##"
direction = "horizontal"
primary = "#a"
secondary = "#b"
"##;
        let parsed: LayoutProfile = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.ratio, 50);
        assert_eq!(parsed.direction, SplitDirection::Horizontal);
    }

    #[test]
    fn split_state_new() {
        let state = SplitState::new(
            SplitDirection::Vertical,
            "libera".to_string(),
            "#rust".to_string(),
        );
        assert_eq!(state.direction, SplitDirection::Vertical);
        assert_eq!(state.secondary_server, "libera");
        assert_eq!(state.secondary_buffer, "#rust");
        assert_eq!(state.ratio, 50);
    }

    #[test]
    fn save_and_load_layout() {
        let dir = test_dir();
        // Override layouts dir by writing directly
        let profile = LayoutProfile {
            direction: SplitDirection::Horizontal,
            primary: "#ops".to_string(),
            secondary: "#alerts".to_string(),
            ratio: 40,
        };
        let path = dir.join("test.toml");
        let toml_str = toml::to_string_pretty(&profile).unwrap();
        fs::write(&path, &toml_str).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let loaded: LayoutProfile = toml::from_str(&contents).unwrap();
        assert_eq!(loaded.direction, SplitDirection::Horizontal);
        assert_eq!(loaded.primary, "#ops");
        assert_eq!(loaded.secondary, "#alerts");
        assert_eq!(loaded.ratio, 40);
    }

    #[test]
    fn list_layouts_from_dir() {
        let dir = test_dir();
        fs::write(dir.join("alpha.toml"), "direction = \"vertical\"\nprimary = \"a\"\nsecondary = \"b\"").unwrap();
        fs::write(dir.join("beta.toml"), "direction = \"vertical\"\nprimary = \"c\"\nsecondary = \"d\"").unwrap();
        fs::write(dir.join("not_toml.txt"), "ignore").unwrap();

        let entries: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext == "toml") {
                    path.file_stem().map(|s| s.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();
        assert!(entries.contains(&"alpha".to_string()));
        assert!(entries.contains(&"beta".to_string()));
        assert!(!entries.contains(&"not_toml".to_string()));
    }
}
