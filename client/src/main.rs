use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use worker::game::{GameEvent, NodeCoord};
use worker::raft::storage::GameEventRequest;
use worker::{bootstrap_cluster, generate_node_id, join_cluster, NodeRegistry, RaftNode};

/// Local player context - tracks which player this client represents
#[derive(Debug, Clone)]
pub struct PlayerContext {
    pub player_id: u64,
    pub player_name: String,
    pub capital_coord: NodeCoord,
}

/// Client state shared across HTTP handlers
#[derive(Clone)]
pub struct ClientState {
    pub raft_node: Arc<RaftNode>,
    pub player_context: Arc<RwLock<Option<PlayerContext>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== CamHack Client Starting ===\n");

    // Step 1: Generate client ID
    let client_id = std::env::var("CLIENT_ID")
        .unwrap_or_else(|_| format!("client-{}", std::process::id()));
    println!("Client ID: {}", client_id);

    // Step 2: Get own IP from ECS metadata
    println!("\n[1/6] Discovering IP address from ECS metadata...");
    let my_ip = worker::metadata::get_task_ip().await?;
    println!("✓ IP address: {}", my_ip);

    // Step 3: Get task ARN from ECS metadata
    println!("\n[2/6] Getting task ARN from ECS metadata...");
    let task_arn = worker::metadata::get_task_arn().await?;
    println!("✓ Task ARN: {}", task_arn);

    // Step 4: Get game ID from environment
    let game_id = std::env::var("GAME_ID")
        .unwrap_or_else(|_| "default-game".to_string());
    println!("\n[3/7] Game ID: {}", game_id);

    // Step 5: Register with master and get peer
    println!("\n[4/7] Registering with master...");
    let peer = worker::registry::register_and_get_peer(client_id.clone(), task_arn, my_ip.clone(), game_id).await?;

    // Step 6: Initialize Raft node (same as worker)
    println!("\n[5/7] Initializing Raft node...");
    let node_id = generate_node_id();
    let registry = NodeRegistry::new();

    let raft_node = if let Some(peer_info) = peer {
        // Join existing cluster
        join_cluster(node_id, my_ip.clone(), peer_info, registry).await?
    } else {
        // Bootstrap new cluster
        bootstrap_cluster(node_id, my_ip.clone(), registry).await?
    };

    // Step 7: Initialize local player
    println!("\n[6/7] Initializing local player...");
    let player_id = generate_player_id();
    let player_name = std::env::var("PLAYER_NAME")
        .unwrap_or_else(|_| format!("Player{}", player_id % 1000));

    // Find a random unoccupied coordinate
    let capital_coord = find_random_unoccupied_coord(&raft_node).await?;
    println!("✓ Assigned capital position: ({}, {})", capital_coord.q, capital_coord.r);

    // Submit PlayerJoin event to Raft cluster
    let join_event = GameEvent::PlayerJoin {
        player_id,
        name: player_name.clone(),
        capital_coord,
        node_ip: my_ip.clone(),
        timestamp: current_timestamp(),
    };

    println!("Submitting PlayerJoin event to cluster...");
    let request = GameEventRequest { event: join_event };
    match raft_node.raft.client_write(request).await {
        Ok(response) => {
            println!("✓ Player joined successfully at log index {}", response.log_id.index);
        }
        Err(e) => {
            eprintln!("Failed to join game: {}", e);
            return Err(e.into());
        }
    }

    // Store player context
    let player_context = Arc::new(RwLock::new(Some(PlayerContext {
        player_id,
        player_name: player_name.clone(),
        capital_coord,
    })));

    // Step 8: Start HTTP API server for player actions
    println!("\n[7/7] Starting HTTP API server...");
    let client_state = ClientState {
        raft_node: raft_node.clone(),
        player_context: player_context.clone(),
    };

    let api_addr = "0.0.0.0:8080";
    tokio::spawn(async move {
        if let Err(e) = start_api_server(client_state, api_addr.to_string()).await {
            eprintln!("HTTP API server error: {}", e);
        }
    });

    println!("\n=== Client Ready ===");
    println!("  Client ID: {}", client_id);
    println!("  Node ID: {}", node_id);
    println!("  Player ID: {}", player_id);
    println!("  Player Name: {}", player_name);
    println!("  Capital: ({}, {})", capital_coord.q, capital_coord.r);
    println!("  IP: {}", my_ip);
    println!("  Raft Port: 5000");
    println!("  HTTP API Port: 8080");
    println!("===================\n");

    // Main loop - show status periodically
    let mut tick_count = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        tick_count += 1;

        let is_leader = raft_node.is_leader().await;
        let storage = raft_node.storage.read().await;
        let state_machine_arc = storage.state_machine();
        drop(storage);
        let sm = state_machine_arc.read().await;

        // Get player-specific stats
        let player = sm.game_state.players.get(&player_id);
        let owned_nodes_count = sm.game_state.get_player_nodes(player_id).len();
        let is_alive = player.map(|p| p.alive).unwrap_or(false);

        drop(sm);

        println!(
            "[Tick {}] {} | Leader: {} | Alive: {} | Nodes: {}",
            tick_count, player_name, is_leader, is_alive, owned_nodes_count
        );
    }
}

/// Generate a unique player ID
fn generate_player_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Get current timestamp in microseconds
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Find a random unoccupied coordinate for the capital
async fn find_random_unoccupied_coord(raft_node: &Arc<RaftNode>) -> Result<NodeCoord> {
    let storage = raft_node.storage.read().await;
    let state_machine_arc = storage.state_machine();
    drop(storage);
    let sm = state_machine_arc.read().await;

    // Try random coordinates in a grid range
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Try up to 100 times to find an unoccupied spot
    for _ in 0..100 {
        let q = rng.gen_range(-10..=10);
        let r = rng.gen_range(-10..=10);
        let coord = NodeCoord::new(q, r);

        if !sm.game_state.nodes.contains_key(&coord) {
            return Ok(coord);
        }
    }

    // Fallback: use timestamp-based coordinate if all random attempts failed
    let timestamp = current_timestamp();
    let q = ((timestamp % 20) as i32) - 10;
    let r = ((timestamp / 20 % 20) as i32) - 10;
    Ok(NodeCoord::new(q, r))
}

/// Start the HTTP API server for player actions
async fn start_api_server(state: ClientState, addr: String) -> Result<()> {
    use axum::{
        extract::{
            ws::{Message, WebSocket},
            State, WebSocketUpgrade,
        },
        response::Response,
        routing::{get, post},
        Json, Router,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct PlayerStatusResponse {
        player_id: u64,
        player_name: String,
        capital_coord: NodeCoord,
        alive: bool,
        owned_nodes: usize,
        is_leader: bool,
    }

    #[derive(Serialize)]
    struct NodeInfo {
        coord: NodeCoord,
        node_type: String,
        capacity: u64,
        current_target: Option<NodeCoord>,
    }

    #[derive(Deserialize)]
    struct AttackRequest {
        target_q: i32,
        target_r: i32,
        node_q: Option<i32>,
        node_r: Option<i32>,
    }

    // GET /my/status - Get local player status
    async fn get_player_status(
        State(state): State<ClientState>,
    ) -> Json<PlayerStatusResponse> {
        let player_ctx = state.player_context.read().await;
        let ctx = player_ctx.as_ref().unwrap();

        let storage = state.raft_node.storage.read().await;
        let sm_arc = storage.state_machine();
        drop(storage);
        let sm = sm_arc.read().await;

        let player = sm.game_state.players.get(&ctx.player_id);
        let owned_nodes = sm.game_state.get_player_nodes(ctx.player_id);
        let is_alive = player.map(|p| p.alive).unwrap_or(false);

        Json(PlayerStatusResponse {
            player_id: ctx.player_id,
            player_name: ctx.player_name.clone(),
            capital_coord: ctx.capital_coord,
            alive: is_alive,
            owned_nodes: owned_nodes.len(),
            is_leader: state.raft_node.is_leader().await,
        })
    }

    // GET /my/nodes - Get all nodes owned by local player
    async fn get_player_nodes(
        State(state): State<ClientState>,
    ) -> Json<Vec<NodeInfo>> {
        let player_ctx = state.player_context.read().await;
        let ctx = player_ctx.as_ref().unwrap();

        let storage = state.raft_node.storage.read().await;
        let sm_arc = storage.state_machine();
        drop(storage);
        let sm = sm_arc.read().await;

        let nodes = sm.game_state.get_player_nodes(ctx.player_id);
        let node_infos: Vec<NodeInfo> = nodes
            .iter()
            .map(|node| NodeInfo {
                coord: node.coord,
                node_type: format!("{:?}", node.node_type),
                capacity: node.capacity,
                current_target: node.current_target,
            })
            .collect();

        Json(node_infos)
    }

    // POST /my/attack - Set attack target for a node
    async fn set_attack_target(
        State(state): State<ClientState>,
        Json(req): Json<AttackRequest>,
    ) -> Result<Json<String>, String> {
        let player_ctx = state.player_context.read().await;
        let ctx = player_ctx.as_ref().unwrap();
        let target_coord = NodeCoord::new(req.target_q, req.target_r);

        // If no node specified, use the capital
        let node_coord = if let (Some(q), Some(r)) = (req.node_q, req.node_r) {
            NodeCoord::new(q, r)
        } else {
            ctx.capital_coord
        };

        // Verify player owns the node
        let storage = state.raft_node.storage.read().await;
        let sm_arc = storage.state_machine();
        drop(storage);
        let sm = sm_arc.read().await;

        if let Some(node) = sm.game_state.nodes.get(&node_coord) {
            if node.owner_id != ctx.player_id {
                return Err("You don't own this node".to_string());
            }
        } else {
            return Err("Node not found".to_string());
        }

        drop(sm);

        // Submit SetNodeTarget event
        let event = GameEvent::SetNodeTarget {
            node_coord,
            target_coord: Some(target_coord),
            timestamp: current_timestamp(),
        };

        let request = GameEventRequest { event };
        match state.raft_node.raft.client_write(request).await {
            Ok(_) => Ok(Json("Attack target set successfully".to_string())),
            Err(e) => Err(format!("Failed to set attack target: {}", e)),
        }
    }

    // WebSocket handler for real-time updates
    async fn websocket_handler(
        State(state): State<ClientState>,
        ws: WebSocketUpgrade,
    ) -> Response {
        ws.on_upgrade(|socket| handle_websocket(socket, state))
    }

    async fn handle_websocket(mut socket: WebSocket, state: ClientState) {
        #[derive(Serialize)]
        struct StateUpdate {
            log_index: u64,
            event_count: usize,
            player_count: usize,
            node_count: usize,
            alive_players: usize,
            latest_event: Option<String>,
        }

        let mut last_log_index = 0u64;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Get current state
                    let storage = state.raft_node.storage.read().await;
                    let sm_arc = storage.state_machine();
                    drop(storage);
                    let sm = sm_arc.read().await;

                    // Check if state has changed
                    let current_log_index = sm.last_applied_log_index;
                    if current_log_index != last_log_index {
                        last_log_index = current_log_index;

                        let latest_event = sm.events.last().map(|e| format!("{:?}", e));
                        let update = StateUpdate {
                            log_index: current_log_index,
                            event_count: sm.events.len(),
                            player_count: sm.game_state.players.len(),
                            node_count: sm.game_state.nodes.len(),
                            alive_players: sm.game_state.players.values().filter(|p| p.alive).count(),
                            latest_event,
                        };
                        drop(sm);

                        // Send update to client
                        let json = match serde_json::to_string(&update) {
                            Ok(j) => j,
                            Err(_) => break,
                        };

                        match socket.send(Message::Text(json)).await {
                            Ok(_) => {},
                            Err(_) => break, // Client disconnected
                        }
                    }
                }

                msg = socket.recv() => {
                    match msg {
                        Some(Ok(Message::Close(_))) => break,
                        Some(Ok(_)) => {}, // Ignore other messages
                        Some(Err(_)) => break,
                        None => break,
                    }
                }
            }
        }
    }

    // Build router
    let app = Router::new()
        .route("/my/status", get(get_player_status))
        .route("/my/nodes", get(get_player_nodes))
        .route("/my/attack", post(set_attack_target))
        .route("/ws", get(websocket_handler))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("✓ HTTP API listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
