use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeybindingMode {
    #[default]
    Emacs,
    Vi,
    Custom,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct KeybindingsConfig {
    #[serde(default)]
    pub mode: KeybindingMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_emacs() {
        let config = KeybindingsConfig::default();
        assert_eq!(config.mode, KeybindingMode::Emacs);
    }

    #[test]
    fn deserialize_vi_mode() {
        let toml_str = r#"mode = "vi""#;
        let config: KeybindingsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, KeybindingMode::Vi);
    }

    #[test]
    fn deserialize_emacs_mode() {
        let toml_str = r#"mode = "emacs""#;
        let config: KeybindingsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, KeybindingMode::Emacs);
    }

    #[test]
    fn deserialize_empty_defaults_emacs() {
        let toml_str = "";
        let config: KeybindingsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, KeybindingMode::Emacs);
    }
}
