use std::net::SocketAddr;
use std::path::Path;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use super::{DccEvent, DccOffer};

const BUFFER_SIZE: usize = 8192;
const PROGRESS_INTERVAL: u64 = 32768; // Report progress every 32KB

/// Receive a file via DCC SEND (we are the receiver, connecting to sender).
pub async fn receive_file(
    id: u64,
    offer: &DccOffer,
    path: &Path,
    resume_pos: u64,
    event_tx: mpsc::Sender<DccEvent>,
) -> Result<(), String> {
    let addr = SocketAddr::new(offer.ip, offer.port);

    let mut stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("Failed to connect to {}:{} — {}", offer.ip, offer.port, e))?;

    let mut file = if resume_pos > 0 {
        tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(path)
            .await
            .map_err(|e| format!("Failed to open file for resume: {}", e))?
    } else {
        File::create(path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?
    };

    let mut bytes_received = resume_pos;
    let total = offer.size;
    let mut buf = vec![0u8; BUFFER_SIZE];
    let mut last_report = bytes_received;

    loop {
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if n == 0 {
            break;
        }

        file.write_all(&buf[..n])
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        bytes_received += n as u64;

        // Send DCC acknowledgment (4-byte big-endian total bytes received)
        let ack = (bytes_received as u32).to_be_bytes();
        let _ = stream.write_all(&ack).await;

        // Report progress periodically
        if bytes_received - last_report >= PROGRESS_INTERVAL {
            let _ = event_tx
                .send(DccEvent::Progress {
                    id,
                    bytes: bytes_received,
                    total,
                })
                .await;
            last_report = bytes_received;
        }

        if total > 0 && bytes_received >= total {
            break;
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;

    let _ = event_tx.send(DccEvent::Complete { id }).await;
    Ok(())
}

/// Send a file via DCC SEND (we are the sender, listening for receiver).
pub async fn send_file(
    id: u64,
    path: &Path,
    listener: TcpListener,
    resume_pos: u64,
    event_tx: mpsc::Sender<DccEvent>,
) -> Result<(), String> {
    let (mut stream, _addr) = listener
        .accept()
        .await
        .map_err(|e| format!("Accept error: {}", e))?;

    let mut file = File::open(path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let metadata = file
        .metadata()
        .await
        .map_err(|e| format!("Metadata error: {}", e))?;
    let total = metadata.len();

    // Seek to resume position
    if resume_pos > 0 {
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(resume_pos))
            .await
            .map_err(|e| format!("Seek error: {}", e))?;
    }

    let mut bytes_sent = resume_pos;
    let mut buf = vec![0u8; BUFFER_SIZE];
    let mut last_report = bytes_sent;

    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if n == 0 {
            break;
        }

        stream
            .write_all(&buf[..n])
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        bytes_sent += n as u64;

        if bytes_sent - last_report >= PROGRESS_INTERVAL {
            let _ = event_tx
                .send(DccEvent::Progress {
                    id,
                    bytes: bytes_sent,
                    total,
                })
                .await;
            last_report = bytes_sent;
        }
    }

    let _ = event_tx.send(DccEvent::Complete { id }).await;
    Ok(())
}

/// Bind a TCP listener on a port within the configured range.
pub async fn bind_listener(port_range: (u16, u16)) -> Result<(TcpListener, u16), String> {
    for port in port_range.0..=port_range.1 {
        match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => return Ok((listener, port)),
            Err(_) => continue,
        }
    }
    Err(format!(
        "No available port in range {}-{}",
        port_range.0, port_range.1
    ))
}

/// Expand ~ in download directory path.
pub fn expand_download_dir(dir: &str) -> std::path::PathBuf {
    if dir.starts_with("~/") {
        if let Some(home) = dirs_home() {
            return home.join(&dir[2..]);
        }
    }
    std::path::PathBuf::from(dir)
}

fn dirs_home() -> Option<std::path::PathBuf> {
    directories::UserDirs::new().map(|d| d.home_dir().to_path_buf())
}
