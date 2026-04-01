pub mod transfer;
pub mod chat;
pub mod xdcc;

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Type of DCC connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DccType {
    Send,
    Chat,
}

/// A parsed DCC offer from a CTCP message.
#[derive(Debug, Clone)]
pub struct DccOffer {
    pub id: u64,
    pub from: String,
    pub server: String,
    pub dcc_type: DccType,
    pub filename: Option<String>,
    pub size: u64,
    pub ip: IpAddr,
    pub port: u16,
    pub token: Option<String>,
    pub passive: bool,
}

/// State of a DCC transfer.
#[derive(Debug, Clone)]
pub enum DccTransferState {
    Pending,
    Connecting,
    Active {
        bytes_transferred: u64,
        total: u64,
    },
    Complete,
    Failed(String),
    Cancelled,
}

/// A tracked DCC transfer (send or receive).
#[derive(Debug, Clone)]
pub struct DccTransfer {
    pub id: u64,
    pub offer: DccOffer,
    pub state: DccTransferState,
    pub started_at: Option<Instant>,
    pub path: Option<PathBuf>,
    /// True if we are the sender.
    pub outgoing: bool,
}

impl DccTransfer {
    pub fn from_offer(offer: DccOffer) -> Self {
        let id = offer.id;
        Self {
            id,
            offer,
            state: DccTransferState::Pending,
            started_at: None,
            path: None,
            outgoing: false,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            DccTransferState::Pending | DccTransferState::Connecting | DccTransferState::Active { .. }
        )
    }

    pub fn progress_percent(&self) -> Option<f64> {
        if let DccTransferState::Active { bytes_transferred, total } = &self.state {
            if *total > 0 {
                return Some(*bytes_transferred as f64 / *total as f64 * 100.0);
            }
        }
        None
    }
}

/// DCC progress event sent from transfer tasks back to the main loop.
#[derive(Debug, Clone)]
pub enum DccEvent {
    Progress { id: u64, bytes: u64, total: u64 },
    Complete { id: u64 },
    Failed { id: u64, error: String },
    ChatMessage { id: u64, text: String },
    ChatDisconnected { id: u64 },
}

/// Parse a CTCP DCC command string into a DccOffer.
/// Expected formats:
///   DCC SEND filename ip port size [token]
///   DCC CHAT chat ip port [token]
///   DCC RESUME filename port position [token]
///   DCC ACCEPT filename port position [token]
pub fn parse_dcc_ctcp(ctcp_params: &str, from: &str, server: &str) -> Option<DccCtcpMessage> {
    let parts: Vec<&str> = ctcp_params.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let dcc_cmd = parts[0].to_uppercase();

    match dcc_cmd.as_str() {
        "SEND" => {
            // DCC SEND filename ip port size [token]
            if parts.len() < 5 {
                return None;
            }
            let filename = parts[1].trim_matches('"').to_string();
            let ip = parse_dcc_ip(parts[2])?;
            let port: u16 = parts[3].parse().ok()?;
            let size: u64 = parts[4].parse().unwrap_or(0);
            let token = parts.get(5).map(|s| s.to_string());
            let passive = port == 0;

            Some(DccCtcpMessage::Offer(DccOffer {
                id: next_id(),
                from: from.to_string(),
                server: server.to_string(),
                dcc_type: DccType::Send,
                filename: Some(filename),
                size,
                ip,
                port,
                token,
                passive,
            }))
        }
        "CHAT" => {
            // DCC CHAT chat ip port [token]
            let ip = parse_dcc_ip(parts[2])?;
            let port: u16 = parts[3].parse().ok()?;
            let token = parts.get(4).map(|s| s.to_string());
            let passive = port == 0;

            Some(DccCtcpMessage::Offer(DccOffer {
                id: next_id(),
                from: from.to_string(),
                server: server.to_string(),
                dcc_type: DccType::Chat,
                filename: None,
                size: 0,
                ip,
                port,
                token,
                passive,
            }))
        }
        "RESUME" => {
            // DCC RESUME filename port position [token]
            if parts.len() < 4 {
                return None;
            }
            let filename = parts[1].trim_matches('"').to_string();
            let port: u16 = parts[2].parse().ok()?;
            let position: u64 = parts[3].parse().ok()?;
            let token = parts.get(4).map(|s| s.to_string());

            Some(DccCtcpMessage::Resume {
                filename,
                port,
                position,
                token,
            })
        }
        "ACCEPT" => {
            // DCC ACCEPT filename port position [token]
            if parts.len() < 4 {
                return None;
            }
            let filename = parts[1].trim_matches('"').to_string();
            let port: u16 = parts[2].parse().ok()?;
            let position: u64 = parts[3].parse().ok()?;
            let token = parts.get(4).map(|s| s.to_string());

            Some(DccCtcpMessage::Accept {
                filename,
                port,
                position,
                token,
            })
        }
        _ => None,
    }
}

/// Parsed DCC CTCP message types.
#[derive(Debug, Clone)]
pub enum DccCtcpMessage {
    Offer(DccOffer),
    Resume {
        filename: String,
        port: u16,
        position: u64,
        token: Option<String>,
    },
    Accept {
        filename: String,
        port: u16,
        position: u64,
        token: Option<String>,
    },
}

/// Parse a DCC IP address. DCC uses either:
/// - A decimal-encoded 32-bit integer (e.g., "3232235876" = 192.168.1.100)
/// - A dotted IPv4 string (some modern clients)
fn parse_dcc_ip(s: &str) -> Option<IpAddr> {
    // Try dotted notation first
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(ip);
    }
    // Try integer notation (standard DCC format)
    let n: u32 = s.parse().ok()?;
    Some(IpAddr::V4(std::net::Ipv4Addr::from(n)))
}

/// Encode an IPv4 address as a DCC integer.
pub fn encode_dcc_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            let n = u32::from_be_bytes(octets);
            n.to_string()
        }
        IpAddr::V6(_) => ip.to_string(),
    }
}

/// Format a file size for display.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dcc_ip_integer() {
        let ip = parse_dcc_ip("3232235876").unwrap();
        assert_eq!(ip, IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)));
    }

    #[test]
    fn parse_dcc_ip_dotted() {
        let ip = parse_dcc_ip("192.168.1.100").unwrap();
        assert_eq!(ip, IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)));
    }

    #[test]
    fn encode_dcc_ip_v4() {
        let ip = IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(encode_dcc_ip(ip), "3232235876");
    }

    #[test]
    fn parse_dcc_send_offer() {
        let msg = parse_dcc_ctcp("SEND file.txt 3232235876 4000 1024", "alice", "libera");
        let msg = msg.unwrap();
        match msg {
            DccCtcpMessage::Offer(offer) => {
                assert_eq!(offer.dcc_type, DccType::Send);
                assert_eq!(offer.filename, Some("file.txt".to_string()));
                assert_eq!(offer.port, 4000);
                assert_eq!(offer.size, 1024);
                assert!(!offer.passive);
                assert_eq!(offer.from, "alice");
            }
            _ => panic!("Expected Offer"),
        }
    }

    #[test]
    fn parse_dcc_send_passive() {
        let msg = parse_dcc_ctcp("SEND file.txt 3232235876 0 1024 token123", "bob", "efnet");
        let msg = msg.unwrap();
        match msg {
            DccCtcpMessage::Offer(offer) => {
                assert!(offer.passive);
                assert_eq!(offer.token, Some("token123".to_string()));
                assert_eq!(offer.port, 0);
            }
            _ => panic!("Expected Offer"),
        }
    }

    #[test]
    fn parse_dcc_chat_offer() {
        let msg = parse_dcc_ctcp("CHAT chat 3232235876 5000", "carol", "libera");
        let msg = msg.unwrap();
        match msg {
            DccCtcpMessage::Offer(offer) => {
                assert_eq!(offer.dcc_type, DccType::Chat);
                assert_eq!(offer.filename, None);
                assert_eq!(offer.port, 5000);
            }
            _ => panic!("Expected Offer"),
        }
    }

    #[test]
    fn parse_dcc_resume() {
        let msg = parse_dcc_ctcp("RESUME file.txt 4000 512", "alice", "libera");
        let msg = msg.unwrap();
        match msg {
            DccCtcpMessage::Resume { filename, port, position, .. } => {
                assert_eq!(filename, "file.txt");
                assert_eq!(port, 4000);
                assert_eq!(position, 512);
            }
            _ => panic!("Expected Resume"),
        }
    }

    #[test]
    fn parse_dcc_accept() {
        let msg = parse_dcc_ctcp("ACCEPT file.txt 4000 512", "alice", "libera");
        let msg = msg.unwrap();
        match msg {
            DccCtcpMessage::Accept { filename, port, position, .. } => {
                assert_eq!(filename, "file.txt");
                assert_eq!(port, 4000);
                assert_eq!(position, 512);
            }
            _ => panic!("Expected Accept"),
        }
    }

    #[test]
    fn parse_dcc_filename_no_spaces() {
        // DCC SEND with simple filename (no spaces)
        let msg = parse_dcc_ctcp("SEND myfile.txt 3232235876 4000 2048", "alice", "libera");
        let msg = msg.unwrap();
        match msg {
            DccCtcpMessage::Offer(offer) => {
                assert_eq!(offer.filename, Some("myfile.txt".to_string()));
                assert_eq!(offer.size, 2048);
            }
            _ => panic!("Expected Offer"),
        }
    }

    #[test]
    fn format_size_display() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.00 GB");
    }

    #[test]
    fn dcc_transfer_progress() {
        let offer = DccOffer {
            id: 1,
            from: "alice".to_string(),
            server: "libera".to_string(),
            dcc_type: DccType::Send,
            filename: Some("test.txt".to_string()),
            size: 1000,
            ip: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port: 4000,
            token: None,
            passive: false,
        };
        let mut transfer = DccTransfer::from_offer(offer);
        assert!(transfer.is_active());
        assert_eq!(transfer.progress_percent(), None);

        transfer.state = DccTransferState::Active {
            bytes_transferred: 500,
            total: 1000,
        };
        assert_eq!(transfer.progress_percent(), Some(50.0));

        transfer.state = DccTransferState::Complete;
        assert!(!transfer.is_active());
    }
}
