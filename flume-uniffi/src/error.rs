/// All FFI fallible methods surface a single error type. Variants map to
/// the major subsystems so Swift can switch exhaustively.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FlumeError {
    #[error("vault: {0}")]
    Vault(String),
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
