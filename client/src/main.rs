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
    pub game_id: String,
}

/// Client state shared across HTTP handlers
#[derive(Clone)]
pub struct ClientState {
    pub raft_node: Arc<RwLock<Option<Arc<RaftNode>>>>,
    pub player_context: Arc<RwLock<Option<PlayerContext>>>,
    pub master_url: Arc<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== CamHack Client Starting ===\n");

    // Get master URL from environment
    let master_url = std::env::var("MASTER_URL").expect("no master url set");
    println!("Master URL: {}", master_url);

    // Create client state with no initial Raft node or player
    let client_state = ClientState {
        raft_node: Arc::new(RwLock::new(None)),
        player_context: Arc::new(RwLock::new(None)),
        master_url: Arc::new(master_url),
    };

    // Start HTTP API server
    println!("\nStarting HTTP API server...");
    let api_addr = "0.0.0.0:8080";

    println!("\n=== Client Ready ===");
    println!("  HTTP API Port: 8080");
    println!("  Status: Not joined to any game");
    println!("  Call POST /join to join a game");
    println!("===================\n");

    // Start server and block
    start_api_server(client_state, api_addr.to_string()).await?;

    Ok(())
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

/// Wait for Raft leader election to complete
async fn wait_for_leader(raft_node: &Arc<RaftNode>, timeout: std::time::Duration) -> Result<u64> {
    use std::time::Instant;
    use tokio::time::sleep;

    let start = Instant::now();
    let poll_interval = std::time::Duration::from_millis(100);

    loop {
        // Check if we've exceeded the timeout
        if start.elapsed() >= timeout {
            return Err(anyhow::anyhow!(
                "Timeout waiting for leader election after {:?}", timeout
            ));
        }

        // Get current Raft metrics to check for leader
        let metrics = raft_node.raft.metrics().borrow().clone();

        if let Some(leader_id) = metrics.current_leader {
            println!("✓ Leader elected: node {}", leader_id);
            return Ok(leader_id);
        }

        // No leader yet, wait and retry
        sleep(poll_interval).await;
    }
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
        http::StatusCode,
        response::Response,
        routing::{get, post},
        Json, Router,
    };
    use serde::{Deserialize, Serialize};
    use tower_http::cors::CorsLayer;
    use tower_http::services::ServeDir;

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
        current_target: Option<String>,
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
    ) -> Result<Json<PlayerStatusResponse>, String> {
        // Check if joined
        let raft_node = state.raft_node.read().await;
        let raft_node = raft_node.as_ref()
            .ok_or("Not joined to any game. Call POST /join first".to_string())?;

        let player_ctx = state.player_context.read().await;
        let ctx = player_ctx.as_ref()
            .ok_or("Player context not initialized".to_string())?;

        let storage = raft_node.storage.read().await;
        let sm_arc = storage.state_machine();
        drop(storage);
        let sm = sm_arc.read().await;

        let player = sm.game_state.players.get(&ctx.player_id);
        let owned_nodes = sm.game_state.get_player_nodes(ctx.player_id);
        let is_alive = player.map(|p| p.alive).unwrap_or(false);
        let is_leader = raft_node.is_leader().await;

        Ok(Json(PlayerStatusResponse {
            player_id: ctx.player_id,
            player_name: ctx.player_name.clone(),
            capital_coord: ctx.capital_coord,
            alive: is_alive,
            owned_nodes: owned_nodes.len(),
            is_leader,
        }))
    }

    // GET /my/nodes - Get all nodes owned by local player
    async fn get_player_nodes(
        State(state): State<ClientState>,
    ) -> Result<Json<Vec<NodeInfo>>, String> {
        // Check if joined
        let raft_node = state.raft_node.read().await;
        let raft_node = raft_node.as_ref()
            .ok_or("Not joined to any game. Call POST /join first".to_string())?;

        let player_ctx = state.player_context.read().await;
        let ctx = player_ctx.as_ref()
            .ok_or("Player context not initialized".to_string())?;

        let storage = raft_node.storage.read().await;
        let sm_arc = storage.state_machine();
        drop(storage);
        let sm = sm_arc.read().await;

        let nodes = sm.game_state.get_player_nodes(ctx.player_id);
        let node_infos: Vec<NodeInfo> = nodes
            .iter()
            .map(|node| NodeInfo {
                coord: node.coord,
                node_type: format!("{:?}", node.node_type),
                current_target: node.current_target.as_ref().map(|t| format!("{:?}", t)),
            })
            .collect();

        Ok(Json(node_infos))
    }

    // GET /game/state - Get full game state (for frontend visualization)
    async fn get_game_state(
        State(state): State<ClientState>,
    ) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
        // Check if joined
        let raft_node = state.raft_node.read().await;
        let raft_node = raft_node.as_ref()
            .ok_or_else(|| (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Not joined to any game. Call POST /join first"}))
            ))?;

        let storage = raft_node.storage.read().await;
        let sm_arc = storage.state_machine();
        drop(storage);
        let sm = sm_arc.read().await;

        // Serialize full game state for frontend
        let game_state_json = serde_json::json!({
            "players": sm.game_state.players.iter().map(|(id, p)| {
                // Count nodes owned by this player
                let node_count = sm.game_state.nodes.values()
                    .filter(|n| n.owner_id == *id)
                    .count();

                serde_json::json!({
                    "player_id": id,  // Fixed: was "id", now "player_id"
                    "name": &p.name,
                    "capital_coord": {
                        "q": p.capital_coord.q,
                        "r": p.capital_coord.r
                    },
                    "alive": p.alive,
                    "join_time": p.join_time,
                    "node_count": node_count  // Added: node count for UI
                })
            }).collect::<Vec<_>>(),
            "nodes": sm.game_state.nodes.iter().map(|(coord, node)| {
                // Properly serialize current_target as JSON object
                let current_target_json = node.current_target.as_ref().map(|t| match t {
                    worker::game::AttackTarget::Coordinate(target_coord) => {
                        serde_json::json!({
                            "q": target_coord.q,
                            "r": target_coord.r
                        })
                    },
                    worker::game::AttackTarget::Player(player_id) => {
                        serde_json::json!({
                            "player_id": player_id
                        })
                    },
                });

                // Get metrics for this node if available
                let metrics = sm.game_state.node_metrics.get(coord);

                serde_json::json!({
                    "coord": {
                        "q": coord.q,
                        "r": coord.r
                    },
                    "owner_id": node.owner_id,
                    "current_target": current_target_json,
                    "bandwidth_in": metrics.map(|m| m.bandwidth_in),
                    "packet_loss": metrics.map(|m| m.packet_loss),
                })
            }).collect::<Vec<_>>(),
            "total_events": sm.events.len()
        });

        Ok(Json(game_state_json))
    }

    // POST /events - Submit a custom game event (for advanced frontend features)
    async fn submit_event(
        State(state): State<ClientState>,
        Json(event_json): Json<serde_json::Value>,
    ) -> Result<Json<String>, String> {
        // Check if joined
        let raft_node = state.raft_node.read().await;
        let raft_node = raft_node.as_ref()
            .ok_or("Not joined to any game. Call POST /join first".to_string())?;

        // Parse event from JSON (simplified for now - only supports SetNodeTarget)
        let event: GameEvent = serde_json::from_value(event_json)
            .map_err(|e| format!("Failed to parse event: {}", e))?;

        let request = GameEventRequest { event };
        raft_node.raft.client_write(request).await
            .map_err(|e| format!("Failed to submit event: {}", e))?;

        Ok(Json("Event submitted successfully".to_string()))
    }

    // POST /my/attack - Set attack target for a node
    async fn set_attack_target(
        State(state): State<ClientState>,
        Json(req): Json<AttackRequest>,
    ) -> Result<Json<String>, String> {
        // Check if joined
        let raft_node = state.raft_node.read().await;
        let raft_node = raft_node.as_ref()
            .ok_or("Not joined to any game. Call POST /join first".to_string())?;

        let player_ctx = state.player_context.read().await;
        let ctx = player_ctx.as_ref()
            .ok_or("Player context not initialized".to_string())?;

        let target_coord = NodeCoord::new(req.target_q, req.target_r);

        // If no node specified, use the capital
        let node_coord = if let (Some(q), Some(r)) = (req.node_q, req.node_r) {
            NodeCoord::new(q, r)
        } else {
            ctx.capital_coord
        };

        // Verify player owns the node
        let storage = raft_node.storage.read().await;
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

        // Check adjacency - target must be adjacent to attacking node
        if !node_coord.is_adjacent(&target_coord) {
            drop(sm);
            return Err("Target must be adjacent to your node".to_string());
        }

        drop(sm);

        // Submit SetNodeTarget event
        let event = GameEvent::SetNodeTarget {
            node_coord,
            target: Some(worker::game::AttackTarget::Coordinate(target_coord)),
            timestamp: current_timestamp(),
        };

        let request = GameEventRequest { event };
        match raft_node.raft.client_write(request).await {
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

        // Check if joined
        {
            let raft_node = state.raft_node.read().await;
            if raft_node.is_none() {
                let _ = socket.send(Message::Text(
                    serde_json::json!({"error": "Not joined to any game. Call POST /join first"}).to_string()
                )).await;
                let _ = socket.close().await;
                return;
            }
        }

        let mut last_log_index = 0u64;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Check if still joined
                    let raft_node_guard = state.raft_node.read().await;
                    let Some(raft_node) = raft_node_guard.as_ref() else {
                        break;
                    };

                    // Get current state
                    let storage = raft_node.storage.read().await;
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

    // GET /discover - Discover available games from master
    async fn discover_games(
        State(state): State<ClientState>,
    ) -> Result<Json<serde_json::Value>, String> {
        let client = reqwest::Client::new();
        let master_url = state.master_url.as_str();

        let response = client
            .get(format!("{}/games", master_url))
            .send()
            .await
            .map_err(|e| format!("Failed to contact master: {}", e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(Json(response))
    }

    // GET /status - Get client join status
    async fn get_status(
        State(state): State<ClientState>,
    ) -> Json<serde_json::Value> {
        let player_ctx = state.player_context.read().await;
        let raft_node = state.raft_node.read().await;

        if let (Some(ctx), Some(_)) = (player_ctx.as_ref(), raft_node.as_ref()) {
            serde_json::json!({
                "joined": true,
                "player_id": ctx.player_id,
                "player_name": ctx.player_name,
                "game_id": ctx.game_id,
                "capital_coord": { "q": ctx.capital_coord.q, "r": ctx.capital_coord.r }
            })
        } else {
            serde_json::json!({
                "joined": false
            })
        }.into()
    }

    // POST /join - Join a game
    #[derive(Deserialize)]
    struct JoinRequest {
        game_id: String,
        player_name: String,
    }

    async fn join_game(
        State(state): State<ClientState>,
        Json(req): Json<JoinRequest>,
    ) -> Result<Json<String>, String> {
        // Check if already joined
        {
            let raft_node = state.raft_node.read().await;
            if raft_node.is_some() {
                return Err("Already joined to a game".to_string());
            }
        }

        println!("\n=== Joining Game: {} ===", req.game_id);

        // Get client ID
        let client_id = std::env::var("CLIENT_ID")
            .unwrap_or_else(|_| format!("client-{}", std::process::id()));

        // Get IP and task ARN from ECS metadata
        let my_ip = worker::metadata::get_task_ip().await
            .map_err(|e| format!("Failed to get IP: {}", e))?;
        let task_arn = worker::metadata::get_task_arn().await
            .map_err(|e| format!("Failed to get task ARN: {}", e))?;

        // Register with master and get peer
        let peer = worker::registry::register_and_get_peer(
            client_id,
            task_arn,
            my_ip.clone(),
            req.game_id.clone(),
        ).await
            .map_err(|e| format!("Failed to register with master: {}", e))?;

        // Initialize Raft node
        let node_id = generate_node_id();
        let registry = NodeRegistry::new();

        let raft_node = if let Some(peer_info) = peer {
            join_cluster(node_id, my_ip.clone(), peer_info, registry).await
        } else {
            bootstrap_cluster(node_id, my_ip.clone(), registry).await
        }.map_err(|e| format!("Failed to initialize Raft: {}", e))?;

        // Wait for leader election to complete before proceeding
        println!("Waiting for Raft leader election...");
        wait_for_leader(&raft_node, std::time::Duration::from_secs(10)).await
            .map_err(|e| format!("Leader election failed: {}", e))?;

        // Initialize player
        let player_id = generate_player_id();
        let capital_coord = find_random_unoccupied_coord(&raft_node).await
            .map_err(|e| format!("Failed to find capital position: {}", e))?;

        // Submit PlayerJoin event
        let join_event = GameEvent::PlayerJoin {
            player_id,
            name: req.player_name.clone(),
            capital_coord,
            node_ip: my_ip,
            is_client: true,  // This is a client (player's laptop)
            timestamp: current_timestamp(),
        };

        let event_request = GameEventRequest { event: join_event };
        raft_node.raft.client_write(event_request).await
            .map_err(|e| format!("Failed to submit join event: {}", e))?;

        // Store state
        let player_ctx = PlayerContext {
            player_id,
            player_name: req.player_name.clone(),
            capital_coord,
            game_id: req.game_id.clone(),
        };

        *state.player_context.write().await = Some(player_ctx);
        *state.raft_node.write().await = Some(raft_node);

        println!("✓ Successfully joined game: {}", req.game_id);

        // Spawn capital worker for this player
        println!("Spawning capital worker at ({}, {})...", capital_coord.q, capital_coord.r);
        let client = reqwest::Client::new();
        let spawn_result = client
            .post(format!("{}/spawn_single_node", state.master_url.as_str()))
            .json(&serde_json::json!({
                "game_id": req.game_id,
                "is_capital": true,
                "q": capital_coord.q,
                "r": capital_coord.r
            }))
            .send()
            .await;

        match spawn_result {
            Ok(resp) => {
                if resp.status().is_success() {
                    println!("✓ Capital worker spawned successfully");
                } else {
                    eprintln!("⚠ Failed to spawn capital worker: {}", resp.status());
                }
            }
            Err(e) => {
                eprintln!("⚠ Failed to spawn capital worker: {}", e);
            }
        }

        Ok(Json(format!("Successfully joined game {} as {}", req.game_id, req.player_name)))
    }

    // WebSocket handler for final kill attacks (10-second client kill)
    async fn finalkill_handler(
        State(_state): State<ClientState>,
        ws: WebSocketUpgrade,
    ) -> Response {
        ws.on_upgrade(handle_finalkill_websocket)
    }

    async fn handle_finalkill_websocket(mut socket: WebSocket) {
        println!("[FinalKill] Attacker connected, receiving flood data...");
        let mut bytes_received = 0u64;

        // Receive data until connection closes
        while let Some(msg) = socket.recv().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    bytes_received += data.len() as u64;
                }
                Ok(Message::Close(_)) => {
                    println!("[FinalKill] Connection closed, total bytes: {}", bytes_received);
                    break;
                }
                Ok(_) => {
                    // Ignore other message types
                }
                Err(e) => {
                    eprintln!("[FinalKill] Error receiving: {}", e);
                    break;
                }
            }
        }

        println!("[FinalKill] Disconnected, total received: {} bytes", bytes_received);
    }

    // Build router
    let app = Router::new()
        .route("/discover", get(discover_games))
        .route("/status", get(get_status))
        .route("/join", post(join_game))
        .route("/my/status", get(get_player_status))
        .route("/my/nodes", get(get_player_nodes))
        .route("/my/attack", post(set_attack_target))
        .route("/game/state", get(get_game_state))
        .route("/events", post(submit_event))
        .route("/ws", get(websocket_handler))
        .route("/finalkill", get(finalkill_handler))
        .nest_service("/", ServeDir::new("static").append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())  // Enable CORS for frontend
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("✓ HTTP API listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
