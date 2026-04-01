use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

/// A unified async stream that wraps either a plain TCP or TLS connection.
pub enum Transport {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl AsyncRead for Transport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Transport::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Transport::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Transport::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Transport::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Transport::Plain(s) => Pin::new(s).poll_flush(cx),
            Transport::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Transport::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Transport::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Connect to a server using TLS.
pub async fn connect_tls(address: &str, port: u16) -> Result<Transport, ConnectionError> {
    let tcp = TcpStream::connect((address, port))
        .await
        .map_err(ConnectionError::Tcp)?;

    // Ensure the ring crypto provider is installed (rustls 0.23 requires this)
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(tls_config));
    let server_name = rustls::pki_types::ServerName::try_from(address.to_string())
        .map_err(|e| ConnectionError::Tls(e.to_string()))?;

    let tls_stream = connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| ConnectionError::Tls(e.to_string()))?;

    Ok(Transport::Tls(tls_stream))
}

/// Connect to a server using plain TCP (no encryption).
pub async fn connect_plain(address: &str, port: u16) -> Result<Transport, ConnectionError> {
    let tcp = TcpStream::connect((address, port))
        .await
        .map_err(ConnectionError::Tcp)?;
    Ok(Transport::Plain(tcp))
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("TCP connection failed: {0}")]
    Tcp(std::io::Error),
    #[error("TLS handshake failed: {0}")]
    Tls(String),
    #[error("registration failed: {0}")]
    Registration(String),
    #[error("SASL authentication failed: {0}")]
    Sasl(String),
    #[error("connection closed by server")]
    ServerClosed,
    #[error("ping timeout")]
    PingTimeout,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
