use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

/// UDP attack packet sent to flood target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpAttackPacket {
    pub seq: u64,            // Sequence number
    pub timestamp: u64,       // Unix timestamp (microseconds)
    pub payload: Vec<u8>,     // 1KB payload
}

/// ACK packet sent back to attacker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpAckPacket {
    pub ack_seq: u64,       // Highest contiguous sequence received
    pub received_count: u64, // Total packets received
}

/// Shared state for tracking packet loss
#[derive(Clone)]
pub struct PacketLossTracker {
    pub sent: Arc<AtomicU64>,
    pub acked: Arc<AtomicU64>,
}

impl PacketLossTracker {
    pub fn new() -> Self {
        Self {
            sent: Arc::new(AtomicU64::new(0)),
            acked: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn calculate_loss(&self) -> f32 {
        let sent = self.sent.load(Ordering::Relaxed);
        let acked = self.acked.load(Ordering::Relaxed);

        if sent == 0 {
            0.0
        } else {
            ((sent - acked.min(sent)) as f32) / (sent as f32)
        }
    }
}

/// UDP responder - receives attack packets and sends ACKs
/// Runs on port 8081
pub async fn udp_responder(
    bytes_received: Arc<AtomicU64>,
) -> Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:8081").await?;
    println!("[UDP] Responder listening on port 8081");

    let mut buf = [0u8; 2048];
    let mut sequences: HashSet<u64> = HashSet::new();
    let mut last_ack = Instant::now();
    let mut packets_received = 0u64;

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, peer)) => {
                // Track bytes
                bytes_received.fetch_add(len as u64, Ordering::Relaxed);

                // Try to parse packet
                if let Ok(packet) = bincode::deserialize::<UdpAttackPacket>(&buf[..len]) {
                    sequences.insert(packet.seq);
                    packets_received += 1;

                    // Send ACK every 100ms
                    if last_ack.elapsed() > Duration::from_millis(100) {
                        let ack = UdpAckPacket {
                            ack_seq: *sequences.iter().max().unwrap_or(&0),
                            received_count: packets_received,
                        };

                        if let Ok(ack_bytes) = bincode::serialize(&ack) {
                            let _ = socket.send_to(&ack_bytes, peer).await;
                        }

                        last_ack = Instant::now();
                    }
                }
            }
            Err(e) => {
                eprintln!("[UDP] Responder error: {}", e);
            }
        }
    }
}

/// UDP attacker - sends attack packets to target
pub async fn udp_attacker(
    target_ip: String,
    tracker: PacketLossTracker,
    mut stop_signal: broadcast::Receiver<()>,
) -> Result<()> {
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let target: SocketAddr = format!("{}:8081", target_ip).parse()?;

    println!("[UDP] Starting attack on {}", target);

    // Spawn ACK receiver
    let socket_clone = socket.clone();
    let tracker_clone = tracker.clone();
    tokio::spawn(async move {
        if let Err(e) = ack_receiver(socket_clone, tracker_clone).await {
            eprintln!("[UDP] ACK receiver error: {}", e);
        }
    });

    let mut seq = 0u64;

    loop {
        tokio::select! {
            // Stop signal received
            _ = stop_signal.recv() => {
                println!("[UDP] Attack stopped, final loss: {:.2}%", tracker.calculate_loss() * 100.0);
                break;
            }

            // Send packet
            _ = async {
                let packet = UdpAttackPacket {
                    seq,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                    payload: vec![0u8; 1024],
                };

                if let Ok(bytes) = bincode::serialize(&packet) {
                    let _ = socket.send_to(&bytes, target).await;
                    tracker.sent.fetch_add(1, Ordering::Relaxed);
                    seq += 1;
                }

                Ok::<_, anyhow::Error>(())
            } => {}
        }
    }

    Ok(())
}

/// ACK receiver - listens for ACKs from target and updates packet loss
async fn ack_receiver(
    socket: Arc<UdpSocket>,
    tracker: PacketLossTracker,
) -> Result<()> {
    let mut buf = [0u8; 256];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, _peer)) => {
                if let Ok(ack) = bincode::deserialize::<UdpAckPacket>(&buf[..len]) {
                    tracker.acked.store(ack.received_count, Ordering::Relaxed);
                }
            }
            Err(e) => {
                eprintln!("[UDP] ACK receiver error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
