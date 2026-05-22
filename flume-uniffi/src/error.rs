use flume_core::config::vault::VaultError;

/// All FFI fallible methods surface a single error type. Variants map to
/// the major subsystems so Swift can switch exhaustively. Specific failure
/// modes that drive distinct UI (e.g. wrong passphrase) get their own variant;
/// everything else collapses into a stringified `*(String)` variant.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FlumeError {
    #[error("vault: {0}")]
    Vault(String),
    #[error("vault: wrong passphrase")]
    VaultWrongPassphrase,
    #[error("vault: not initialized")]
    VaultNotInitialized,
    #[error("vault: locked")]
    VaultLocked,
    #[error("vault: already initialized")]
    VaultAlreadyInitialized,
    #[error("config: {0}")]
    Config(String),
    #[error("network: {0}")]
    Network(String),
    #[error("connection: {0}")]
    Connection(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("not implemented")]
    NotImplemented,
}

impl From<VaultError> for FlumeError {
    fn from(err: VaultError) -> Self {
        match err {
            VaultError::WrongPassphrase | VaultError::Decryption => {
                FlumeError::VaultWrongPassphrase
            }
            VaultError::NotFound => FlumeError::VaultNotInitialized,
            other => FlumeError::Vault(other.to_string()),
        }
    }
}
