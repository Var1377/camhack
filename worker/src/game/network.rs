use super::events::{GameEvent, NodeCoord};
use super::state::GameState;
use super::udp::{udp_responder, udp_attacker, PacketLossTracker};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, RwLock};

/// Manages hybrid UDP/WebSocket attack connections and metrics
/// Capacity determined by actual network infrastructure
pub struct NetworkManager {
    my_coord: Option<NodeCoord>,
    /// Active attack connections (target_coord -> connection handle)
    active_attacks: Arc<RwLock<HashMap<NodeCoord, AttackConnection>>>,
    /// Packet loss trackers for each active UDP attack
    packet_trackers: Arc<RwLock<HashMap<NodeCoord, PacketLossTracker>>>,
    /// Total bytes received from all attacks (UDP + WebSocket)
    bytes_received: Arc<AtomicU64>,
    /// Last measurement time for bandwidth calculation
    last_measurement: Arc<RwLock<SystemTime>>,
    /// UDP socket for sending attack packets to workers
    udp_socket: Option<Arc<UdpSocket>>,
}

/// Represents an active UDP attack connection to a grid node
struct AttackConnection {
    target_coord: NodeCoord,
    target_ip: String,
    /// Handle to stop the connection
    stop_signal: tokio::sync::broadcast::Sender<()>,
}

impl NetworkManager {
    pub fn new() -> Self {
        // Start UDP responder to receive incoming attack packets
        // This runs independently and doesn't need to know our coordinate
        let bytes_received = Arc::new(AtomicU64::new(0));
        let bytes_received_clone = bytes_received.clone();

        tokio::spawn(async move {
            if let Err(e) = udp_responder(bytes_received_clone).await {
                eprintln!("[Network] UDP responder error: {}", e);
            }
        });

        Self {
            my_coord: None,
            active_attacks: Arc::new(RwLock::new(HashMap::new())),
            packet_trackers: Arc::new(RwLock::new(HashMap::new())),
            bytes_received,
            last_measurement: Arc::new(RwLock::new(SystemTime::now())),
            udp_socket: None,
        }
    }

    /// Start a UDP attack on a target node
    async fn start_udp_attack(
        &self,
        target_coord: NodeCoord,
        target_ip: String,
    ) -> Result<()> {
        let (stop_tx, stop_rx) = broadcast::channel(1);

        // Create packet loss tracker for this attack
        let tracker = PacketLossTracker::new();

        // Store tracker
        let mut trackers = self.packet_trackers.write().await;
        trackers.insert(target_coord, tracker.clone());
        drop(trackers);

        let target_ip_for_spawn = target_ip.clone();

        // Spawn UDP attacker task
        tokio::spawn(async move {
            println!("[Network] Starting UDP attack on {:?} at {}", target_coord, target_ip_for_spawn);

            if let Err(e) = udp_attacker(target_ip_for_spawn, tracker, stop_rx).await {
                eprintln!("[Network] UDP attacker error on {:?}: {}", target_coord, e);
            }
        });

        // Store connection handle
        let connection = AttackConnection {
            target_coord,
            target_ip,
            stop_signal: stop_tx,
        };

        let mut attacks = self.active_attacks.write().await;
        attacks.insert(target_coord, connection);

        Ok(())
    }

    /// Stop an attack connection
    async fn stop_attack_connection(&self, target_coord: NodeCoord) {
        let mut attacks = self.active_attacks.write().await;
        if let Some(connection) = attacks.remove(&target_coord) {
            // Send stop signal (ignore if receiver is already dropped)
            let _ = connection.stop_signal.send(());
            println!("[Network] Stopped attack on {:?}", target_coord);
        }

        // Clean up packet tracker
        let mut trackers = self.packet_trackers.write().await;
        trackers.remove(&target_coord);
    }

    /// Get current metrics for all active attacks
    pub async fn get_metrics(&self) -> Vec<GameEvent> {
        let my_coord = match self.my_coord {
            Some(c) => c,
            None => return Vec::new(),
        };

        let attacks = self.active_attacks.read().await;
        if attacks.is_empty() {
            return Vec::new();
        }

        // Calculate bandwidth over the last measurement period
        let now = SystemTime::now();
        let mut last_measurement = self.last_measurement.write().await;
        let duration = now.duration_since(*last_measurement).unwrap_or(Duration::from_secs(1));
        *last_measurement = now;

        let bytes = self.bytes_received.swap(0, Ordering::Relaxed);
        let bandwidth_in = (bytes as f64 / duration.as_secs_f64()) as u64;

        // Calculate average packet loss across all active attacks
        let trackers = self.packet_trackers.read().await;
        let packet_loss = if trackers.is_empty() {
            0.0
        } else {
            let total_loss: f32 = trackers.values().map(|t| t.calculate_loss()).sum();
            total_loss / trackers.len() as f32
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Report metrics for this node (being attacked)
        vec![GameEvent::NodeMetricsReport {
            node_coord: my_coord,
            bandwidth_in,
            packet_loss,
            timestamp,
        }]
    }

    /// Update attacks based on current game state
    /// This is called when this node is BEING ATTACKED
    pub async fn sync_with_game_state(
        &mut self,
        game_state: &GameState,
        ip_map: &HashMap<NodeCoord, String>,
        my_ip: &str,
    ) {
        // Auto-discover my coordinate if not yet known
        let my_coord = match self.my_coord {
            Some(c) => c,
            None => {
                // Find my coordinate by matching my IP in the IP map
                let found_coord = ip_map
                    .iter()
                    .find(|(_, ip)| ip.as_str() == my_ip)
                    .map(|(coord, _)| *coord);

                match found_coord {
                    Some(coord) => {
                        println!("[Network] Auto-discovered my coordinate: {:?}", coord);
                        self.my_coord = Some(coord);
                        coord
                    }
                    None => {
                        // My IP not in map yet - still initializing
                        return;
                    }
                }
            }
        };

        // Find all nodes that are attacking ME
        let attackers: Vec<(NodeCoord, u64)> = game_state
            .nodes
            .values()
            .filter(|node| node.current_target == Some(super::events::AttackTarget::Coordinate(my_coord)))
            .map(|node| (node.coord, node.owner_id))
            .collect();

        if attackers.is_empty() {
            // Not being attacked, close all connections
            let attacks = self.active_attacks.read().await;
            let to_stop: Vec<NodeCoord> = attacks.keys().copied().collect();
            drop(attacks);

            for target in to_stop {
                self.stop_attack_connection(target).await;
            }
            return;
        }

        // For each attacking coordinate, open a 1-to-1 UDP connection
        for (attacker_coord, _attacker_owner) in &attackers {
            // Check if we already have a connection to this attacker
            let attacks = self.active_attacks.read().await;
            if attacks.contains_key(attacker_coord) {
                continue;
            }
            drop(attacks);

            // Get IP for the attacking node (1-to-1 connection)
            if let Some(target_ip) = ip_map.get(attacker_coord) {
                // Open UDP attack to THIS SPECIFIC attacking node only
                if let Err(e) = self
                    .start_udp_attack(*attacker_coord, target_ip.clone())
                    .await
                {
                    eprintln!("[Network] Failed to start UDP attack: {}", e);
                }
            } else {
                // Attacking node has no IP yet - skip for now
                println!("[Network] Attacker {:?} has no IP yet, skipping", attacker_coord);
            }
        }

        // Stop connections to nodes that are no longer attacking
        let attacker_coords: Vec<NodeCoord> = attackers
            .iter()
            .map(|(coord, _)| *coord)
            .collect();

        let attacks = self.active_attacks.read().await;
        let to_stop: Vec<NodeCoord> = attacks
            .keys()
            .filter(|coord| !attacker_coords.contains(coord))
            .copied()
            .collect();
        drop(attacks);

        for target in to_stop {
            self.stop_attack_connection(target).await;
        }
    }
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}
