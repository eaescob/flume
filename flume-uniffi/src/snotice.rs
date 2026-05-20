//! Snotice rule surface — stubs in M1, implementation in M6.

use crate::{error::FlumeError, FlumeClient};

#[derive(Debug, Clone, uniffi::Record)]
pub struct SnoticeRule {
    /// Stable identifier assigned on add — used for update/remove.
    pub id: String,
    /// Regex pattern matched against the notice text.
    pub pattern: String,
    /// Optional ${1}-style format applied to the matched text.
    pub format: Option<String>,
    /// Optional destination buffer (empty/None routes to server buffer).
    pub buffer: Option<String>,
    /// If true, the notice is dropped instead of routed.
    pub suppress: bool,
    pub enabled: bool,
}

#[uniffi::export]
impl FlumeClient {
    pub fn list_snotice_rules(&self) -> Vec<SnoticeRule> {
        Vec::new()
    }

    pub fn add_snotice_rule(&self, _rule: SnoticeRule) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn remove_snotice_rule(&self, _id: String) {}

    pub fn update_snotice_rule(&self, _rule: SnoticeRule) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn save_snotice_rules(&self) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    /// Compiles the pattern, applies it to `text`, returns the capture groups
    /// (group 0 = full match, then 1..). For the visual rule editor.
    pub fn test_snotice_pattern(
        &self,
        _pattern: String,
        _text: String,
    ) -> Result<Vec<String>, FlumeError> {
        Err(FlumeError::NotImplemented)
    }
}
