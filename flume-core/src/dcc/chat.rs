use std::net::SocketAddr;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use super::DccEvent;

/// Run a DCC CHAT session (we initiated or accepted).
/// Reads lines from the peer and sends them as DccEvent::ChatMessage.
/// Writes lines received from `outgoing_rx` to the peer.
pub async fn run_chat(
    id: u64,
    stream: TcpStream,
    event_tx: mpsc::Sender<DccEvent>,
    mut outgoing_rx: mpsc::Receiver<String>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let event_tx2 = event_tx.clone();
    let read_task = tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            if event_tx2
                .send(DccEvent::ChatMessage {
                    id,
                    text: line,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let write_task = tokio::spawn(async move {
        while let Some(line) = outgoing_rx.recv().await {
            let data = format!("{}\n", line);
            if writer.write_all(data.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    // Wait for either side to finish
    tokio::select! {
        _ = read_task => {},
        _ = write_task => {},
    }

    let _ = event_tx.send(DccEvent::ChatDisconnected { id }).await;
}

/// Connect to a DCC CHAT peer (we are the one connecting).
pub async fn connect_chat(ip: std::net::IpAddr, port: u16) -> Result<TcpStream, String> {
    let addr = SocketAddr::new(ip, port);
    TcpStream::connect(addr)
        .await
        .map_err(|e| format!("Failed to connect DCC CHAT to {}:{} — {}", ip, port, e))
}

/// Accept a DCC CHAT connection (we are listening).
pub async fn accept_chat(listener: TcpListener) -> Result<TcpStream, String> {
    let (stream, _) = listener
        .accept()
        .await
        .map_err(|e| format!("DCC CHAT accept error: {}", e))?;
    Ok(stream)
}
