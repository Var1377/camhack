use super::events::{GameEvent, NodeCoord};
use super::state::GameState;
use anyhow::Result;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// UDP port for game attack packets
pub const GAME_UDP_PORT: u16 = 9000;

/// Attack packet sent from attacker to target
#[derive(Debug, Clone)]
struct AttackPacket {
    seq_number: u64,
    timestamp: u64,
    attacker_coord: NodeCoord,
}

impl AttackPacket {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.seq_number.to_be_bytes());
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes.extend_from_slice(&self.attacker_coord.q.to_be_bytes());
        bytes.extend_from_slice(&self.attacker_coord.r.to_be_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 24 {
            return None;
        }
        let seq_number = u64::from_be_bytes(bytes[0..8].try_into().ok()?);
        let timestamp = u64::from_be_bytes(bytes[8..16].try_into().ok()?);
        let q = i32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let r = i32::from_be_bytes(bytes[20..24].try_into().ok()?);
        Some(Self {
            seq_number,
            timestamp,
            attacker_coord: NodeCoord::new(q, r),
        })
    }
}

/// ACK packet sent from target back to attacker
#[derive(Debug, Clone)]
struct AckPacket {
    seq_number: u64,
    target_coord: NodeCoord,
}

impl AckPacket {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.seq_number.to_be_bytes());
        bytes.extend_from_slice(&self.target_coord.q.to_be_bytes());
        bytes.extend_from_slice(&self.target_coord.r.to_be_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        let seq_number = u64::from_be_bytes(bytes[0..8].try_into().ok()?);
        let q = i32::from_be_bytes(bytes[8..12].try_into().ok()?);
        let r = i32::from_be_bytes(bytes[12..16].try_into().ok()?);
        Some(Self {
            seq_number,
            target_coord: NodeCoord::new(q, r),
        })
    }
}

/// Floods a target with UDP packets and measures packet loss
pub struct PacketFlooder {
    target_ip: String,
    target_coord: NodeCoord,
    attacker_coord: NodeCoord,
    socket: Arc<UdpSocket>,
    packets_sent: Arc<AtomicU64>,
    packets_acked: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

impl PacketFlooder {
    pub fn new(
        target_ip: String,
        target_coord: NodeCoord,
        attacker_coord: NodeCoord,
    ) -> Result<Self> {
        // Bind to any available port for sending
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            target_ip,
            target_coord,
            attacker_coord,
            socket: Arc::new(socket),
            packets_sent: Arc::new(AtomicU64::new(0)),
            packets_acked: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start flooding packets to the target
    pub fn start(&self) {
        self.running.store(true, Ordering::SeqCst);

        // Spawn sender thread
        let socket = self.socket.clone();
        let target_addr = format!("{}:{}", self.target_ip, GAME_UDP_PORT);
        let packets_sent = self.packets_sent.clone();
        let running = self.running.clone();
        let attacker_coord = self.attacker_coord;

        std::thread::spawn(move || {
            let mut seq = 0u64;
            while running.load(Ordering::SeqCst) {
                seq += 1;
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64;

                let packet = AttackPacket {
                    seq_number: seq,
                    timestamp,
                    attacker_coord,
                };

                if let Ok(addr) = target_addr.parse::<SocketAddr>() {
                    let _ = socket.send_to(&packet.to_bytes(), addr);
                    packets_sent.fetch_add(1, Ordering::SeqCst);
                }

                // Send as fast as possible (no sleep)
            }
        });

        // Spawn receiver thread for ACKs
        let socket_clone = self.socket.clone();
        let packets_acked = self.packets_acked.clone();
        let running_clone = self.running.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while running_clone.load(Ordering::SeqCst) {
                match socket_clone.recv_from(&mut buf) {
                    Ok((len, _)) => {
                        if let Some(_ack) = AckPacket::from_bytes(&buf[..len]) {
                            packets_acked.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => {}
                }
            }
        });
    }

    /// Stop flooding
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Calculate packet loss (0.0 to 1.0)
    pub fn get_packet_loss(&self) -> f32 {
        let sent = self.packets_sent.load(Ordering::SeqCst);
        let acked = self.packets_acked.load(Ordering::SeqCst);

        if sent == 0 {
            return 0.0;
        }

        let lost = sent.saturating_sub(acked);
        (lost as f32) / (sent as f32)
    }

    /// Get bytes sent per second (estimate)
    pub fn get_bandwidth(&self) -> u64 {
        let sent = self.packets_sent.load(Ordering::SeqCst);
        // Each packet is ~24 bytes, rough estimate
        sent * 24
    }
}

/// Responds to incoming attack packets with ACKs (rate-limited by capacity)
pub struct PacketResponder {
    my_coord: NodeCoord,
    capacity: u64, // bytes/sec
    socket: Arc<UdpSocket>,
    running: Arc<AtomicBool>,
}

impl PacketResponder {
    pub fn new(my_coord: NodeCoord, capacity: u64, bind_addr: String) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr)?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            my_coord,
            capacity,
            socket: Arc::new(socket),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start responding to packets
    pub fn start(&self) {
        self.running.store(true, Ordering::SeqCst);

        let socket = self.socket.clone();
        let my_coord = self.my_coord;
        let capacity = self.capacity;
        let running = self.running.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            let mut packets_this_second = 0u64;
            let mut second_start = SystemTime::now();

            // Calculate max packets per second based on capacity
            let max_packets_per_second = capacity / 16; // Assume 16 bytes per ACK

            while running.load(Ordering::SeqCst) {
                // Reset counter every second
                if second_start.elapsed().unwrap() >= Duration::from_secs(1) {
                    packets_this_second = 0;
                    second_start = SystemTime::now();
                }

                // If we've hit capacity, skip processing until next second
                if packets_this_second >= max_packets_per_second {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }

                match socket.recv_from(&mut buf) {
                    Ok((len, src_addr)) => {
                        if let Some(attack_packet) = AttackPacket::from_bytes(&buf[..len]) {
                            // Send ACK back
                            let ack = AckPacket {
                                seq_number: attack_packet.seq_number,
                                target_coord: my_coord,
                            };

                            let _ = socket.send_to(&ack.to_bytes(), src_addr);
                            packets_this_second += 1;
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => {}
                }
            }
        });
    }

    /// Stop responding
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Manages all network attacks and metrics reporting
pub struct NetworkManager {
    my_coord: Option<NodeCoord>,
    my_capacity: u64,
    active_attacks: Arc<RwLock<HashMap<NodeCoord, PacketFlooder>>>,
    responder: Option<PacketResponder>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            my_coord: None,
            my_capacity: 0,
            active_attacks: Arc::new(RwLock::new(HashMap::new())),
            responder: None,
        }
    }

    /// Initialize with this node's info
    pub fn initialize(&mut self, my_coord: NodeCoord, my_capacity: u64, my_ip: String) -> Result<()> {
        self.my_coord = Some(my_coord);
        self.my_capacity = my_capacity;

        // Start responder
        let bind_addr = format!("{}:{}", my_ip, GAME_UDP_PORT);
        let responder = PacketResponder::new(my_coord, my_capacity, bind_addr)?;
        responder.start();
        self.responder = Some(responder);

        Ok(())
    }

    /// Start attacking a target
    pub async fn start_attack(
        &self,
        target_coord: NodeCoord,
        target_ip: String,
    ) -> Result<()> {
        let my_coord = self.my_coord.expect("NetworkManager not initialized");

        let flooder = PacketFlooder::new(target_ip, target_coord, my_coord)?;
        flooder.start();

        let mut attacks = self.active_attacks.write().await;
        attacks.insert(target_coord, flooder);

        println!("[Network] Started attack: {:?} -> {:?}", my_coord, target_coord);
        Ok(())
    }

    /// Stop attacking a target
    pub async fn stop_attack(&self, target_coord: NodeCoord) {
        let mut attacks = self.active_attacks.write().await;
        if let Some(flooder) = attacks.remove(&target_coord) {
            flooder.stop();
            println!("[Network] Stopped attack on {:?}", target_coord);
        }
    }

    /// Get current metrics for all active attacks
    pub async fn get_metrics(&self) -> Vec<GameEvent> {
        let attacks = self.active_attacks.read().await;
        let my_coord = match self.my_coord {
            Some(c) => c,
            None => return Vec::new(),
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        attacks
            .iter()
            .map(|(target_coord, flooder)| {
                let packet_loss = flooder.get_packet_loss();
                let bandwidth = flooder.get_bandwidth();

                GameEvent::NodeMetricsReport {
                    node_coord: *target_coord,
                    bandwidth_in: bandwidth,
                    packet_loss,
                    timestamp,
                }
            })
            .collect()
    }

    /// Update attacks based on current game state
    pub async fn sync_with_game_state(&self, game_state: &GameState, ip_map: &HashMap<NodeCoord, String>) {
        let my_coord = match self.my_coord {
            Some(c) => c,
            None => return,
        };

        // Check what we should be attacking
        let should_attack = if let Some(my_node) = game_state.nodes.get(&my_coord) {
            my_node.current_target
        } else {
            None
        };

        // Start new attack if needed
        if let Some(target_coord) = should_attack {
            let attacks = self.active_attacks.read().await;
            if !attacks.contains_key(&target_coord) {
                drop(attacks);
                if let Some(target_ip) = ip_map.get(&target_coord) {
                    let _ = self.start_attack(target_coord, target_ip.clone()).await;
                }
            }
        }

        // Stop attacks that shouldn't be happening
        let attacks = self.active_attacks.read().await;
        let to_stop: Vec<NodeCoord> = attacks
            .keys()
            .filter(|&&target| Some(target) != should_attack)
            .copied()
            .collect();
        drop(attacks);

        for target in to_stop {
            self.stop_attack(target).await;
        }
    }
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}
