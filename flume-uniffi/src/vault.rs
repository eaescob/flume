//! Encrypted secrets vault. Wraps flume-core::config::vault::Vault.
//!
//! State machine: NotInitialized → (init) → Unlocked → (lock) → Locked → (unlock) → Unlocked.
//! Persistence is automatic for mutations (init, set, delete, change_passphrase) —
//! contrast with `save_networks()` in M3 which is explicit.

use flume_core::config::vault::Vault;

use crate::{error::FlumeError, paths, FlumeClient};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum VaultStatus {
    /// No vault file exists on disk.
    NotInitialized,
    /// Vault file exists but isn't loaded into memory.
    Locked,
    /// Vault is loaded; get/set/delete/list are available.
    Unlocked,
}

#[uniffi::export]
impl FlumeClient {
    pub fn vault_status(&self) -> VaultStatus {
        if self.state.lock().expect("state poisoned").vault.is_some() {
            return VaultStatus::Unlocked;
        }
        match paths::vault_path() {
            Some(p) if p.exists() => VaultStatus::Locked,
            _ => VaultStatus::NotInitialized,
        }
    }

    /// Create a brand-new vault and persist it immediately. Errors if a
    /// vault file already exists — callers must `vault_unlock` or move
    /// the existing file out of the way first.
    pub fn vault_init(&self, passphrase: String) -> Result<(), FlumeError> {
        let path = paths::vault_path().ok_or_else(|| {
            FlumeError::Config("could not resolve vault path ($HOME unset?)".to_string())
        })?;
        if path.exists() {
            return Err(FlumeError::VaultAlreadyInitialized);
        }
        let vault = Vault::new(path, passphrase);
        vault.save()?;
        self.state.lock().expect("state poisoned").vault = Some(vault);
        Ok(())
    }

    /// Decrypt and load the on-disk vault with the given passphrase. No-op
    /// if already unlocked. Returns VaultWrongPassphrase on bad passphrase.
    pub fn vault_unlock(&self, passphrase: String) -> Result<(), FlumeError> {
        {
            let state = self.state.lock().expect("state poisoned");
            if state.vault.is_some() {
                return Ok(());
            }
        }
        let path = paths::vault_path().ok_or_else(|| {
            FlumeError::Config("could not resolve vault path ($HOME unset?)".to_string())
        })?;
        let vault = Vault::load(path, passphrase)?;
        self.state.lock().expect("state poisoned").vault = Some(vault);
        Ok(())
    }

    /// Re-encrypt the vault with a new passphrase. Requires the current
    /// passphrase to authenticate — guards against an unlocked-vault attacker
    /// silently rotating the key.
    pub fn vault_change_passphrase(
        &self,
        old: String,
        new: String,
    ) -> Result<(), FlumeError> {
        let path = paths::vault_path().ok_or_else(|| {
            FlumeError::Config("could not resolve vault path ($HOME unset?)".to_string())
        })?;
        // Verify the old passphrase by attempting a fresh load against disk.
        // This is the only way to confirm the caller has the right secret.
        let _ = Vault::load(path.clone(), old)?;

        let mut state = self.state.lock().expect("state poisoned");
        let vault = state.vault.as_mut().ok_or(FlumeError::VaultLocked)?;
        vault.change_passphrase(new);
        vault.save()?;
        Ok(())
    }

    /// Drop the in-memory vault. The on-disk file is untouched.
    pub fn vault_lock(&self) {
        self.state.lock().expect("state poisoned").vault = None;
    }

    /// Read a secret. Returns Ok(None) if the secret doesn't exist;
    /// VaultLocked if the vault isn't unlocked.
    pub fn vault_get(&self, name: String) -> Result<Option<String>, FlumeError> {
        let state = self.state.lock().expect("state poisoned");
        let vault = state.vault.as_ref().ok_or(FlumeError::VaultLocked)?;
        Ok(vault.get(&name).map(|s| s.to_string()))
    }

    pub fn vault_set(&self, name: String, value: String) -> Result<(), FlumeError> {
        let mut state = self.state.lock().expect("state poisoned");
        let vault = state.vault.as_mut().ok_or(FlumeError::VaultLocked)?;
        vault.set(name, value);
        vault.save()?;
        Ok(())
    }

    pub fn vault_delete(&self, name: String) -> Result<(), FlumeError> {
        let mut state = self.state.lock().expect("state poisoned");
        let vault = state.vault.as_mut().ok_or(FlumeError::VaultLocked)?;
        vault.delete(&name);
        vault.save()?;
        Ok(())
    }

    /// Return all secret names (never values), sorted.
    pub fn vault_list(&self) -> Result<Vec<String>, FlumeError> {
        let state = self.state.lock().expect("state poisoned");
        let vault = state.vault.as_ref().ok_or(FlumeError::VaultLocked)?;
        Ok(vault.list().into_iter().map(|s| s.to_string()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Each test points FLUME_MAC_CONFIG_DIR at a fresh tmpdir. `#[serial]`
    /// keeps them from racing on the process-global env var.
    fn with_tmp_dir() -> TempDir {
        let tmp = TempDir::new().expect("tmpdir");
        std::env::set_var("FLUME_MAC_CONFIG_DIR", tmp.path());
        tmp
    }

    #[test]
    #[serial]
    fn full_state_machine() {
        let _tmp = with_tmp_dir();
        let client = FlumeClient::new();

        // Fresh tmpdir → NotInitialized
        assert!(matches!(client.vault_status(), VaultStatus::NotInitialized));

        // Init + auto-unlock
        client.vault_init("hunter2".into()).unwrap();
        assert!(matches!(client.vault_status(), VaultStatus::Unlocked));

        // Re-init should fail
        assert!(matches!(
            client.vault_init("anything".into()),
            Err(FlumeError::VaultAlreadyInitialized),
        ));

        // Set / get / list
        client.vault_set("nick".into(), "alice".into()).unwrap();
        client.vault_set("token".into(), "xyz".into()).unwrap();
        assert_eq!(
            client.vault_get("nick".into()).unwrap(),
            Some("alice".to_string()),
        );
        assert_eq!(
            client.vault_list().unwrap(),
            vec!["nick".to_string(), "token".to_string()],
        );

        // Lock drops in-memory state but leaves disk file
        client.vault_lock();
        assert!(matches!(client.vault_status(), VaultStatus::Locked));
        assert!(matches!(
            client.vault_get("nick".into()),
            Err(FlumeError::VaultLocked),
        ));

        // Wrong passphrase
        assert!(matches!(
            client.vault_unlock("wrong".into()),
            Err(FlumeError::VaultWrongPassphrase),
        ));

        // Correct passphrase restores secrets
        client.vault_unlock("hunter2".into()).unwrap();
        assert_eq!(
            client.vault_get("nick".into()).unwrap(),
            Some("alice".to_string()),
        );

        // Change passphrase with wrong "old" is rejected
        assert!(matches!(
            client.vault_change_passphrase("wrong".into(), "newpass".into()),
            Err(FlumeError::VaultWrongPassphrase),
        ));

        // Change passphrase + verify new one works
        client
            .vault_change_passphrase("hunter2".into(), "newpass".into())
            .unwrap();
        client.vault_lock();
        client.vault_unlock("newpass".into()).unwrap();
        assert_eq!(
            client.vault_get("nick".into()).unwrap(),
            Some("alice".to_string()),
        );

        // Delete removes the secret
        client.vault_delete("nick".into()).unwrap();
        assert_eq!(client.vault_get("nick".into()).unwrap(), None);
    }

    #[test]
    #[serial]
    fn locked_status_for_existing_file_in_fresh_process() {
        let tmp = with_tmp_dir();

        // First client: init + lock
        let c1 = FlumeClient::new();
        c1.vault_init("pw".into()).unwrap();
        c1.vault_set("k".into(), "v".into()).unwrap();
        c1.vault_lock();
        drop(c1);

        // Second client (same disk state, no in-memory vault) → Locked
        std::env::set_var("FLUME_MAC_CONFIG_DIR", tmp.path());
        let c2 = FlumeClient::new();
        assert!(matches!(c2.vault_status(), VaultStatus::Locked));
        c2.vault_unlock("pw".into()).unwrap();
        assert_eq!(c2.vault_get("k".into()).unwrap(), Some("v".to_string()));
    }
}

