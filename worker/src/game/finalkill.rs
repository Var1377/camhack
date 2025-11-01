use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{sleep, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::events::NodeCoord;

/// Manages 10-second final kill attacks on client nodes
/// Uses WebSocket reverse connections (attacker connects to client)
pub struct FinalKillManager {
    /// Active final kill attacks (player_id -> attack handle)
    active_kills: Arc<RwLock<HashMap<u64, FinalKillHandle>>>,
}

/// Handle for an active final kill attack
struct FinalKillHandle {
    player_id: u64,
    client_ip: String,
    start_time: Instant,
    stop_signal: tokio::sync::broadcast::Sender<()>,
}

impl FinalKillManager {
    pub fn new() -> Self {
        Self {
            active_kills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a 10-second final kill attack on a player's client
    /// all_attacker_nodes: all nodes owned by the attacking player
    pub async fn start_final_kill(
        &self,
        player_id: u64,
        client_ip: String,
        all_attacker_nodes: Vec<NodeCoord>,
    ) -> Result<()> {
        // Check if already attacking this player
        let kills = self.active_kills.read().await;
        if kills.contains_key(&player_id) {
            println!("[FinalKill] Already attacking player {}, skipping", player_id);
            return Ok(());
        }
        drop(kills);

        let (stop_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        let handle = FinalKillHandle {
            player_id,
            client_ip: client_ip.clone(),
            start_time: Instant::now(),
            stop_signal: stop_tx.clone(),
        };

        // Store handle
        let mut kills = self.active_kills.write().await;
        kills.insert(player_id, handle);
        drop(kills);

        println!(
            "[FinalKill] Starting 10-second attack on player {} at {}",
            player_id, client_ip
        );

        // Spawn WebSocket connections from each attacker node to the client
        let num_connections = all_attacker_nodes.len();
        for (idx, node_coord) in all_attacker_nodes.into_iter().enumerate() {
            let client_ip_clone = client_ip.clone();
            let mut stop_rx = stop_tx.subscribe();

            tokio::spawn(async move {
                let ws_url = format!("ws://{}:8080/finalkill", client_ip_clone);
                println!(
                    "[FinalKill] Node {:?} ({}/{}) connecting to {}",
                    node_coord,
                    idx + 1,
                    num_connections,
                    ws_url
                );

                // Try to connect
                match connect_async(&ws_url).await {
                    Ok((mut ws_stream, _)) => {
                        println!("[FinalKill] Node {:?} connected, flooding...", node_coord);

                        // Prepare 1KB flood data
                        let flood_data = vec![0u8; 1024];
                        let flood_msg = Message::Binary(flood_data);

                        loop {
                            tokio::select! {
                                // Stop signal received
                                _ = stop_rx.recv() => {
                                    println!("[FinalKill] Node {:?} stopping attack", node_coord);
                                    let _ = ws_stream.close(None).await;
                                    break;
                                }
                                // Send flood data
                                result = ws_stream.send(flood_msg.clone()) => {
                                    match result {
                                        Ok(_) => {
                                            // Continue flooding with no delay (true flood)
                                        }
                                        Err(e) => {
                                            eprintln!("[FinalKill] Node {:?} send error: {}", node_coord, e);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[FinalKill] Node {:?} failed to connect to {}: {}",
                            node_coord, ws_url, e
                        );
                    }
                }
            });
        }

        // Spawn timer task to stop after 10 seconds
        let active_kills = self.active_kills.clone();
        tokio::spawn(async move {
            sleep(Duration::from_secs(10)).await;
            println!("[FinalKill] 10 seconds elapsed, stopping attack on player {}", player_id);

            // Send stop signal and remove handle
            let mut kills = active_kills.write().await;
            if let Some(handle) = kills.remove(&player_id) {
                let _ = handle.stop_signal.send(());
            }
        });

        Ok(())
    }

    /// Stop an active final kill attack early (e.g., if player already eliminated)
    pub async fn stop_final_kill(&self, player_id: u64) {
        let mut kills = self.active_kills.write().await;
        if let Some(handle) = kills.remove(&player_id) {
            println!("[FinalKill] Stopping attack on player {} early", player_id);
            let _ = handle.stop_signal.send(());
        }
    }

    /// Check if currently attacking a player
    pub async fn is_attacking(&self, player_id: u64) -> bool {
        let kills = self.active_kills.read().await;
        kills.contains_key(&player_id)
    }
}

impl Default for FinalKillManager {
    fn default() -> Self {
        Self::new()
    }
}
