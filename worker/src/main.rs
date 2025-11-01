mod game;
mod metadata;
mod raft;
mod registry;

use anyhow::Result;
use game::{GameConfig, GameLogic, NetworkManager};
use raft::storage::GameEventRequest;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Worker Node Starting ===\n");

    // Step 1: Generate worker ID
    let worker_id = std::env::var("WORKER_ID")
        .unwrap_or_else(|_| format!("worker-{}", std::process::id()));
    println!("Worker ID: {}", worker_id);

    // Step 2: Get own IP from ECS metadata
    println!("\n[1/5] Discovering IP address from ECS metadata...");
    let my_ip = metadata::get_task_ip().await?;
    println!("✓ IP address: {}", my_ip);

    // Step 3: Get task ARN from ECS metadata
    println!("\n[2/5] Getting task ARN from ECS metadata...");
    let task_arn = metadata::get_task_arn().await?;
    println!("✓ Task ARN: {}", task_arn);

    // Step 4: Get game ID from environment
    let game_id = std::env::var("GAME_ID")
        .unwrap_or_else(|_| "default-game".to_string());
    println!("\n[3/6] Game ID: {}", game_id);

    // Step 5: Register with master and get peer
    println!("\n[4/6] Registering with master...");
    let peer = registry::register_and_get_peer(worker_id.clone(), task_arn, my_ip.clone(), game_id).await?;

    // Step 6: Initialize Raft node
    println!("\n[5/6] Initializing Raft node...");
    let node_id = raft::generate_node_id();

    // Create node registry for peer address resolution
    let registry = raft::node_registry::NodeRegistry::new();

    let raft_node = if let Some(peer_info) = peer {
        // Join existing cluster
        raft::join_cluster(node_id, my_ip.clone(), peer_info, registry).await?
    } else {
        // Bootstrap new cluster
        raft::bootstrap_cluster(node_id, my_ip.clone(), registry).await?
    };

    // Step 7: Start HTTP API server for event submission
    println!("\n[6/6] Starting HTTP API server...");
    let api_raft = raft_node.raft.clone();
    let api_storage = raft_node.storage.clone();
    let api_addr = format!("0.0.0.0:8080");
    tokio::spawn(async move {
        if let Err(e) = raft::api::start_api_server(api_raft, api_storage, api_addr).await {
            eprintln!("HTTP API server error: {}", e);
        }
    });

    println!("\n[6/6] Worker startup complete!");
    println!("\n=== Worker Node Ready ===");
    println!("  Worker ID: {}", worker_id);
    println!("  Node ID: {}", node_id);
    println!("  IP: {}", my_ip);
    println!("  Is Leader: {}", raft_node.is_leader().await);
    println!("  Raft Port: 5000");
    println!("  HTTP API Port: 8080");
    println!("========================\n");

    // Initialize game logic (used when this node is leader)
    let mut game_logic = GameLogic::new(GameConfig::default());

    // Initialize network manager (for packet flooding and metrics)
    let network_manager = Arc::new(NetworkManager::new());

    // TODO: Auto-join the game or wait for manual join via API
    // For now, NetworkManager will be initialized when the first PlayerJoin
    // event for this worker is processed

    // Main loop - run game logic tick and show status
    let mut tick_count = 0;
    let mut metrics_tick = 0;

    loop {
        sleep(Duration::from_secs(1)).await;
        tick_count += 1;
        metrics_tick += 1;

        let is_leader = raft_node.is_leader().await;

        // Get current game state
        let storage = raft_node.storage.read().await;
        let state_machine_arc = storage.state_machine();
        drop(storage);
        let sm = state_machine_arc.read().await;
        let game_state = sm.game_state.clone();
        drop(sm);

        // Sync network manager with game state (start/stop attacks)
        network_manager.sync_with_game_state(&game_state, &game_state.node_ips).await;

        // Every 5 seconds, submit metrics reports
        if metrics_tick >= 5 {
            metrics_tick = 0;
            let metrics_events = network_manager.get_metrics().await;

            for event in metrics_events {
                let request = GameEventRequest { event: event.clone() };
                match raft_node.raft.client_write(request).await {
                    Ok(_) => {
                        // Metrics submitted successfully
                    }
                    Err(e) => {
                        eprintln!("[Network] Failed to submit metrics: {}", e);
                    }
                }
            }
        }

        // If leader, run game logic tick to check for captures
        if is_leader {
            // Generate capture events based on overload conditions
            let events = game_logic.tick(&game_state);

            // Submit each generated event back to Raft
            for event in events {
                let request = GameEventRequest { event: event.clone() };
                match raft_node.raft.client_write(request).await {
                    Ok(response) => {
                        println!(
                            "[GameLogic] Auto-generated event committed at log index {}",
                            response.log_id.index
                        );
                    }
                    Err(e) => {
                        eprintln!("[GameLogic] Failed to submit capture event: {}", e);
                    }
                }
            }
        }

        // Show status every 30 seconds
        if tick_count % 30 == 0 {
            let storage = raft_node.storage.read().await;
            let state_machine_arc = storage.state_machine();
            drop(storage);
            let sm = state_machine_arc.read().await;

            // Access derived game state
            let event_count = sm.events.len();
            let player_count = sm.game_state.players.len();
            let node_count = sm.game_state.nodes.len();
            let alive_players = sm.game_state.players.values().filter(|p| p.alive).count();
            drop(sm);

            println!(
                "[Tick {}] Worker {} | Leader: {} | Events: {} | Players: {}/{} | Nodes: {}",
                tick_count, worker_id, is_leader, event_count, alive_players, player_count, node_count
            );
        }
    }
}
