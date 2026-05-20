//! Vault surface — stubs in M1, implementation in M2.

use crate::{error::FlumeError, FlumeClient};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum VaultStatus {
    NotInitialized,
    Locked,
    Unlocked,
}

#[uniffi::export]
impl FlumeClient {
    pub fn vault_status(&self) -> VaultStatus {
        VaultStatus::NotInitialized
    }

    pub fn vault_init(&self, _passphrase: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn vault_unlock(&self, _passphrase: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn vault_change_passphrase(
        &self,
        _old: String,
        _new: String,
    ) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn vault_lock(&self) {}

    pub fn vault_get(&self, _name: String) -> Result<Option<String>, FlumeError> {
        Ok(None)
    }

    pub fn vault_set(&self, _name: String, _value: String) -> Result<(), FlumeError> {
        Err(FlumeError::NotImplemented)
    }

    pub fn vault_delete(&self, _name: String) {}

    pub fn vault_list(&self) -> Vec<String> {
        Vec::new()
    }
}
