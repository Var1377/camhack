use super::events::{GameEvent, NodeCoord};
use super::state::GameState;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Manages WebSocket attack connections and metrics
pub struct NetworkManager {
    my_coord: Option<NodeCoord>,
    my_capacity: u64, // bytes/sec
    /// Active attack connections (target_coord -> connection handle)
    active_attacks: Arc<RwLock<HashMap<NodeCoord, AttackConnection>>>,
    /// Total bytes received from all attacks
    bytes_received: Arc<AtomicU64>,
    /// Last measurement time for bandwidth calculation
    last_measurement: Arc<RwLock<SystemTime>>,
}

/// Represents an active WebSocket attack connection to a target node
struct AttackConnection {
    target_coord: NodeCoord,
    target_ip: String,
    /// Handle to stop the connection
    stop_signal: tokio::sync::broadcast::Sender<()>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            my_coord: None,
            my_capacity: 0,
            active_attacks: Arc::new(RwLock::new(HashMap::new())),
            bytes_received: Arc::new(AtomicU64::new(0)),
            last_measurement: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// Initialize with this node's info
    pub fn initialize(&mut self, my_coord: NodeCoord, my_capacity: u64) {
        self.my_coord = Some(my_coord);
        self.my_capacity = my_capacity;
        println!("[Network] Initialized: coord={:?}, capacity={} bytes/s", my_coord, my_capacity);
    }

    /// Start an attack by opening WebSocket connection to target node
    async fn start_attack_connection(
        &self,
        target_coord: NodeCoord,
        target_ip: String,
    ) -> Result<()> {
        let (stop_tx, mut stop_rx) = tokio::sync::broadcast::channel(1);
        let bytes_received = self.bytes_received.clone();
        let target_ip_for_spawn = target_ip.clone();

        // Spawn task to maintain WebSocket connection
        tokio::spawn(async move {
            let ws_url = format!("ws://{}:8080/attack", target_ip_for_spawn);
            println!("[Network] Opening attack WS to {:?} at {}", target_coord, ws_url);

            // Try to connect
            match connect_async(&ws_url).await {
                Ok((mut ws_stream, _)) => {
                    println!("[Network] Connected to {:?}", target_coord);

                    // Receive data from target until stopped
                    loop {
                        tokio::select! {
                            // Check for stop signal
                            _ = stop_rx.recv() => {
                                println!("[Network] Stopping attack on {:?}", target_coord);
                                let _ = ws_stream.close(None).await;
                                break;
                            }
                            // Receive data from WebSocket
                            msg = ws_stream.next() => {
                                match msg {
                                    Some(Ok(Message::Binary(data))) => {
                                        // Count bytes received
                                        bytes_received.fetch_add(data.len() as u64, Ordering::Relaxed);
                                    }
                                    Some(Ok(_)) => {
                                        // Ignore other message types
                                    }
                                    Some(Err(e)) => {
                                        eprintln!("[Network] WebSocket error from {:?}: {}", target_coord, e);
                                        break;
                                    }
                                    None => {
                                        println!("[Network] WebSocket closed by {:?}", target_coord);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Network] Failed to connect to {:?}: {}", target_coord, e);
                }
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

        // Calculate packet loss based on capacity
        let packet_loss = if bandwidth_in > self.my_capacity {
            ((bandwidth_in - self.my_capacity) as f32) / (bandwidth_in as f32)
        } else {
            0.0
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
        &self,
        game_state: &GameState,
        ip_map: &HashMap<NodeCoord, String>,
    ) {
        let my_coord = match self.my_coord {
            Some(c) => c,
            None => return,
        };

        // Find all nodes that are attacking ME
        let attackers: Vec<(NodeCoord, u64)> = game_state
            .nodes
            .values()
            .filter(|node| node.current_target == Some(my_coord))
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

        // For each attacker, open connections to ALL nodes owned by that player
        for (_attacker_coord, attacker_owner) in &attackers {
            let attacker_nodes = game_state
                .nodes
                .values()
                .filter(|n| n.owner_id == *attacker_owner);

            for node in attacker_nodes {
                // Check if we already have a connection to this node
                let attacks = self.active_attacks.read().await;
                if attacks.contains_key(&node.coord) {
                    continue;
                }
                drop(attacks);

                // Get IP for this node
                if let Some(target_ip) = ip_map.get(&node.coord) {
                    // Open WebSocket connection to this node
                    if let Err(e) = self
                        .start_attack_connection(node.coord, target_ip.clone())
                        .await
                    {
                        eprintln!("[Network] Failed to start attack connection: {}", e);
                    }
                }
            }
        }

        // Stop connections to nodes that are no longer part of the attack
        let all_attacker_nodes: Vec<NodeCoord> = attackers
            .iter()
            .flat_map(|(_, owner_id)| {
                game_state
                    .nodes
                    .values()
                    .filter(|n| n.owner_id == *owner_id)
                    .map(|n| n.coord)
                    .collect::<Vec<_>>()
            })
            .collect();

        let attacks = self.active_attacks.read().await;
        let to_stop: Vec<NodeCoord> = attacks
            .keys()
            .filter(|coord| !all_attacker_nodes.contains(coord))
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
