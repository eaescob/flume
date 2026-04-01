use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const NONCE_LEN: usize = 12;
const SALT_LEN: usize = 32;
const KEY_LEN: usize = 32;

/// The on-disk format of the vault file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultFile {
    /// Argon2 salt, base64-encoded.
    salt: String,
    /// AES-256-GCM nonce, base64-encoded.
    nonce: String,
    /// Encrypted + base64-encoded payload (the serialized secrets map).
    data: String,
}

/// An unlocked secrets vault held in memory.
#[derive(Debug, Clone)]
pub struct Vault {
    secrets: HashMap<String, String>,
    path: PathBuf,
    passphrase: String,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("vault file not found")]
    NotFound,
    #[error("wrong passphrase")]
    WrongPassphrase,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("encryption error: {0}")]
    Encryption(String),
    #[error("decryption failed — wrong passphrase or corrupted data")]
    Decryption,
    #[error("argon2 error: {0}")]
    Argon2(String),
}

impl Vault {
    /// Create a new empty vault with the given passphrase.
    pub fn new(path: PathBuf, passphrase: String) -> Self {
        Vault {
            secrets: HashMap::new(),
            path,
            passphrase,
        }
    }

    /// Load and decrypt an existing vault file.
    pub fn load(path: PathBuf, passphrase: String) -> Result<Self, VaultError> {
        if !path.exists() {
            return Err(VaultError::NotFound);
        }

        let contents = std::fs::read_to_string(&path)?;
        let vault_file: VaultFile = toml::from_str(&contents)?;

        let engine = base64::engine::general_purpose::STANDARD;
        let salt = engine.decode(&vault_file.salt)?;
        let nonce_bytes = engine.decode(&vault_file.nonce)?;
        let ciphertext = engine.decode(&vault_file.data)?;

        let key = derive_key(&passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| VaultError::Encryption(e.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| VaultError::Decryption)?;

        let json_str = String::from_utf8(plaintext)
            .map_err(|_| VaultError::Decryption)?;
        let secrets: HashMap<String, String> = serde_json::from_str(&json_str)
            .map_err(|_| VaultError::Decryption)?;

        Ok(Vault {
            secrets,
            path,
            passphrase,
        })
    }

    /// Save the vault to disk, encrypting with the current passphrase.
    pub fn save(&self) -> Result<(), VaultError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let engine = base64::engine::general_purpose::STANDARD;

        let mut salt = vec![0u8; SALT_LEN];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut salt);

        let mut nonce_bytes = vec![0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let key = derive_key(&self.passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| VaultError::Encryption(e.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let json = serde_json::to_string(&self.secrets)
            .map_err(|e| VaultError::Encryption(e.to_string()))?;
        let ciphertext = cipher
            .encrypt(nonce, json.as_bytes())
            .map_err(|e| VaultError::Encryption(e.to_string()))?;

        let vault_file = VaultFile {
            salt: engine.encode(&salt),
            nonce: engine.encode(&nonce_bytes),
            data: engine.encode(&ciphertext),
        };

        let toml_str = toml::to_string_pretty(&vault_file)?;
        std::fs::write(&self.path, toml_str)?;

        Ok(())
    }

    /// Get a secret by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.secrets.get(name).map(|s| s.as_str())
    }

    /// Set a secret.
    pub fn set(&mut self, name: String, value: String) {
        self.secrets.insert(name, value);
    }

    /// Delete a secret. Returns true if it existed.
    pub fn delete(&mut self, name: &str) -> bool {
        self.secrets.remove(name).is_some()
    }

    /// List all secret names (never values).
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.secrets.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Change the vault passphrase. Call save() after to persist.
    pub fn change_passphrase(&mut self, new_passphrase: String) {
        self.passphrase = new_passphrase;
    }

    /// Return the vault file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Derive a 256-bit key from a passphrase using Argon2id.
fn derive_key(passphrase: &str, salt: &[u8]) -> Result<Vec<u8>, VaultError> {
    let mut key = vec![0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| VaultError::Argon2(e.to_string()))?;
    Ok(key)
}

/// Resolve `${secret_name}` references in a string.
///
/// Looks up the vault first, then falls back to environment variables.
/// Returns the original string with all `${...}` references expanded.
/// Unresolved references are left as-is.
pub fn resolve_secrets(input: &str, vault: Option<&Vault>) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find('}') {
            let name = &after_start[..end];
            // Try vault first, then env var
            let value = vault
                .and_then(|v| v.get(name).map(String::from))
                .or_else(|| std::env::var(name).ok());
            match value {
                Some(v) => result.push_str(&v),
                None => {
                    // Leave unresolved reference as-is
                    result.push_str(&rest[start..start + 3 + end]);
                }
            }
            rest = &after_start[end + 1..];
        } else {
            // No closing brace, just copy the rest
            result.push_str(&rest[start..]);
            rest = "";
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn vault_set_get_delete() {
        let mut vault = Vault::new(PathBuf::from("/tmp/test_vault.toml"), "pass".to_string());
        vault.set("key1".to_string(), "value1".to_string());
        assert_eq!(vault.get("key1"), Some("value1"));
        assert_eq!(vault.get("nonexistent"), None);
        assert!(vault.delete("key1"));
        assert_eq!(vault.get("key1"), None);
        assert!(!vault.delete("key1"));
    }

    #[test]
    fn vault_list() {
        let mut vault = Vault::new(PathBuf::from("/tmp/test_vault.toml"), "pass".to_string());
        vault.set("beta".to_string(), "2".to_string());
        vault.set("alpha".to_string(), "1".to_string());
        assert_eq!(vault.list(), vec!["alpha", "beta"]);
    }

    #[test]
    fn vault_encrypt_decrypt_round_trip() {
        let path = PathBuf::from("/tmp/flume_test_vault_roundtrip.toml");
        let _ = std::fs::remove_file(&path);

        let mut vault = Vault::new(path.clone(), "my_passphrase".to_string());
        vault.set("server_pass".to_string(), "s3cret!".to_string());
        vault.set("nickserv".to_string(), "hunter2".to_string());
        vault.save().unwrap();

        let loaded = Vault::load(path.clone(), "my_passphrase".to_string()).unwrap();
        assert_eq!(loaded.get("server_pass"), Some("s3cret!"));
        assert_eq!(loaded.get("nickserv"), Some("hunter2"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vault_wrong_passphrase() {
        let path = PathBuf::from("/tmp/flume_test_vault_wrong_pass.toml");
        let _ = std::fs::remove_file(&path);

        let mut vault = Vault::new(path.clone(), "correct".to_string());
        vault.set("key".to_string(), "value".to_string());
        vault.save().unwrap();

        let result = Vault::load(path.clone(), "wrong".to_string());
        assert!(matches!(result, Err(VaultError::Decryption)));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resolve_secrets_from_vault() {
        let mut vault = Vault::new(PathBuf::from("/tmp/unused"), "p".to_string());
        vault.set("my_pass".to_string(), "hunter2".to_string());

        let result = resolve_secrets("password is ${my_pass} here", Some(&vault));
        assert_eq!(result, "password is hunter2 here");
    }

    #[test]
    fn resolve_secrets_from_env() {
        env::set_var("FLUME_TEST_SECRET_XYZ", "env_value");
        let result = resolve_secrets("${FLUME_TEST_SECRET_XYZ}", None);
        assert_eq!(result, "env_value");
        env::remove_var("FLUME_TEST_SECRET_XYZ");
    }

    #[test]
    fn resolve_secrets_unresolved_left_as_is() {
        let result = resolve_secrets("${nonexistent_var_12345}", None);
        assert_eq!(result, "${nonexistent_var_12345}");
    }

    #[test]
    fn resolve_secrets_no_references() {
        let result = resolve_secrets("plain text", None);
        assert_eq!(result, "plain text");
    }

    #[test]
    fn resolve_secrets_multiple_refs() {
        let mut vault = Vault::new(PathBuf::from("/tmp/unused"), "p".to_string());
        vault.set("a".to_string(), "1".to_string());
        vault.set("b".to_string(), "2".to_string());

        let result = resolve_secrets("${a} and ${b}", Some(&vault));
        assert_eq!(result, "1 and 2");
    }

    #[test]
    fn resolve_secrets_unclosed_brace() {
        let result = resolve_secrets("${unclosed", None);
        assert_eq!(result, "${unclosed");
    }
}
