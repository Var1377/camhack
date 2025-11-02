use crate::game::{GameEvent, NodeCoord, Player, Node};
use crate::game::events::AttackTarget;
use crate::raft::storage::{GameEventRequest, GameRaftTypeConfig};
use anyhow::Result;
use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::SinkExt;
use openraft::Raft;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

/// HTTP API state shared across handlers
#[derive(Clone)]
pub struct ApiState {
    pub raft: Arc<Raft<GameRaftTypeConfig>>,
    pub storage: Arc<tokio::sync::RwLock<crate::raft::storage::MemStorage>>,
    pub master_url: String,
    pub game_id: String,
}

/// Request to submit a new game event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitEventRequest {
    pub event: GameEvent,
}

/// Response from submitting an event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitEventResponse {
    pub success: bool,
    pub message: String,
    pub log_index: Option<u64>,
}

/// Response for querying events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<GameEvent>,
    pub count: usize,
}

/// Status response showing cluster state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub node_id: u64,
    pub is_leader: bool,
    pub current_leader: Option<u64>,
    pub current_term: u64,
    pub event_count: usize,
}

// ============= Game Command Types =============

/// Request to join the game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGameRequest {
    pub player_name: String,
    pub node_ip: String,  // IP address of the joining worker/node
}

/// Response from joining the game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGameResponse {
    pub success: bool,
    pub message: String,
    pub player_id: Option<u64>,
    pub capital_coord: Option<NodeCoord>,
}

/// Request to attack a neighbor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackRequest {
    pub node_coord: NodeCoord,
    pub target_coord: NodeCoord,
}

/// Request to stop attacking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopAttackRequest {
    pub node_coord: NodeCoord,
}

/// Generic success response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub success: bool,
    pub message: String,
}

/// Game state snapshot for queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateResponse {
    pub players: Vec<PlayerInfo>,
    pub nodes: Vec<NodeInfo>,
    pub total_events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub player_id: u64,
    pub name: String,
    pub capital_coord: NodeCoord,
    pub alive: bool,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub coord: NodeCoord,
    pub owner_id: u64,
    pub current_target: Option<AttackTarget>,
    pub bandwidth_in: Option<u64>,
    pub packet_loss: Option<f32>,
}

/// Create the HTTP API router
pub fn create_router(state: ApiState) -> Router {
    Router::new()
        // Legacy event submission endpoints
        .route("/events", post(submit_event))
        .route("/events", get(get_events))
        .route("/status", get(get_status))
        // Game command endpoints
        .route("/game/join", post(handle_join_game))
        .route("/game/attack", post(handle_attack_command))
        .route("/game/stop-attack", post(handle_stop_attack))
        .route("/game/state", get(handle_get_game_state))
        // WebSocket attack endpoint
        .route("/attack", get(handle_attack))
        .layer(CorsLayer::permissive())  // Enable CORS for frontend
        .with_state(state)
}

/// Submit a game event for consensus
async fn submit_event(
    State(state): State<ApiState>,
    Json(req): Json<SubmitEventRequest>,
) -> impl IntoResponse {
    // Check if this node is the leader
    let metrics = state.raft.metrics().borrow().clone();

    if metrics.current_leader != Some(metrics.id) {
        // Not the leader - return error with leader info
        let response = SubmitEventResponse {
            success: false,
            message: format!(
                "Not the leader. Current leader: {:?}",
                metrics.current_leader
            ),
            log_index: None,
        };
        return (StatusCode::SERVICE_UNAVAILABLE, Json(response));
    }

    // Submit to Raft for consensus
    let request = GameEventRequest {
        event: req.event.clone(),
    };

    match state.raft.client_write(request).await {
        Ok(response) => {
            let log_index = response.log_id.index;
            let response = SubmitEventResponse {
                success: true,
                message: format!("Event committed at log index {}", log_index),
                log_index: Some(log_index),
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            let response = SubmitEventResponse {
                success: false,
                message: format!("Failed to commit event: {}", e),
                log_index: None,
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
        }
    }
}

/// Get all committed events
async fn get_events(State(state): State<ApiState>) -> impl IntoResponse {
    let storage = state.storage.read().await;
    let state_machine = storage.state_machine();
    drop(storage);

    let sm = state_machine.read().await;
    let events = sm.events.clone();
    let count = events.len();
    drop(sm);

    let response = EventsResponse { events, count };
    (StatusCode::OK, Json(response))
}

/// Get cluster status
async fn get_status(State(state): State<ApiState>) -> impl IntoResponse {
    let metrics = state.raft.metrics().borrow().clone();

    let storage = state.storage.read().await;
    let state_machine = storage.state_machine();
    drop(storage);

    let sm = state_machine.read().await;
    let event_count = sm.events.len();
    drop(sm);

    let response = StatusResponse {
        node_id: metrics.id,
        is_leader: metrics.current_leader == Some(metrics.id),
        current_leader: metrics.current_leader,
        current_term: metrics.current_term,
        event_count,
    };

    (StatusCode::OK, Json(response))
}

// ============= Game Command Handlers =============

/// Handle player joining the game
async fn handle_join_game(
    State(state): State<ApiState>,
    Json(req): Json<JoinGameRequest>,
) -> impl IntoResponse {
    // Check if this node is the leader
    let metrics = state.raft.metrics().borrow().clone();
    if metrics.current_leader != Some(metrics.id) {
        let response = JoinGameResponse {
            success: false,
            message: format!("Not the leader. Current leader: {:?}", metrics.current_leader),
            player_id: None,
            capital_coord: None,
        };
        return (StatusCode::SERVICE_UNAVAILABLE, Json(response));
    }

    // Generate player ID from timestamp
    let player_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;

    // Generate capital coordinates (simple spiral pattern)
    let storage = state.storage.read().await;
    let sm_arc = storage.state_machine();
    drop(storage);
    let sm = sm_arc.read().await;
    let player_count = sm.game_state.players.len() as i32;
    drop(sm);

    let capital_coord = NodeCoord::new(player_count * 3, 0);

    // Create PlayerJoin event
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let event = GameEvent::PlayerJoin {
        player_id,
        name: req.player_name.clone(),
        capital_coord,
        node_ip: req.node_ip,
        is_client: false,  // Worker node, not client
        timestamp,
    };

    let request = GameEventRequest { event };

    match state.raft.client_write(request).await {
        Ok(_) => {
            let response = JoinGameResponse {
                success: true,
                message: format!("Player {} joined successfully", req.player_name),
                player_id: Some(player_id),
                capital_coord: Some(capital_coord),
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            let response = JoinGameResponse {
                success: false,
                message: format!("Failed to join game: {}", e),
                player_id: None,
                capital_coord: None,
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
        }
    }
}

/// Handle attack command
async fn handle_attack_command(
    State(state): State<ApiState>,
    Json(req): Json<AttackRequest>,
) -> impl IntoResponse {
    // Check if this node is the leader
    let metrics = state.raft.metrics().borrow().clone();
    if metrics.current_leader != Some(metrics.id) {
        let response = CommandResponse {
            success: false,
            message: format!("Not the leader. Current leader: {:?}", metrics.current_leader),
        };
        return (StatusCode::SERVICE_UNAVAILABLE, Json(response));
    }

    // Validate: check if nodes exist and are neighbors
    let storage = state.storage.read().await;
    let sm_arc = storage.state_machine();
    drop(storage);
    let sm = sm_arc.read().await;

    if !sm.game_state.nodes.contains_key(&req.node_coord) {
        drop(sm);
        let response = CommandResponse {
            success: false,
            message: format!("Attacker node {:?} does not exist", req.node_coord),
        };
        return (StatusCode::BAD_REQUEST, Json(response));
    }

    // Check if they're neighbors
    if !req.node_coord.is_adjacent(&req.target_coord) {
        drop(sm);
        let response = CommandResponse {
            success: false,
            message: "Nodes are not adjacent".to_string(),
        };
        return (StatusCode::BAD_REQUEST, Json(response));
    }

    // If target doesn't exist, trigger lazy initialization
    if !sm.game_state.nodes.contains_key(&req.target_coord) {
        println!("[Lazy Init] Target {:?} doesn't exist, initializing neighbors...", req.target_coord);

        // Get all 6 neighbors of the target
        let neighbors = req.target_coord.neighbors();
        let mut nodes_to_init = Vec::new();

        // Find neighbors that don't exist in game state
        for neighbor in &neighbors {
            if !sm.game_state.nodes.contains_key(neighbor) {
                nodes_to_init.push(*neighbor);
            }
        }

        println!("[Lazy Init] Found {} uninitialized neighbors to spawn", nodes_to_init.len());
        drop(sm);

        // Submit NodeInitializationStarted events for all neighbors to init
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for coord in &nodes_to_init {
            let event = GameEvent::NodeInitializationStarted {
                node_coord: *coord,
                owner_id: 0,  // Neutral/unowned
                timestamp,
            };
            let request = GameEventRequest { event };

            // Submit event (fire and forget, errors logged)
            if let Err(e) = state.raft.client_write(request).await {
                eprintln!("[Lazy Init] Failed to submit initialization event for {:?}: {}", coord, e);
            } else {
                println!("[Lazy Init] Submitted initialization event for {:?}", coord);
            }
        }

        // Call master API to spawn EC2 instances (async, don't wait)
        let master_url = state.master_url.clone();
        let game_id = state.game_id.clone();
        tokio::spawn(async move {
            for coord in nodes_to_init {
                println!("[Lazy Init] Spawning EC2 instance for {:?}...", coord);
                if let Err(e) = spawn_node_on_master(&master_url, &game_id, coord.q, coord.r, false).await {
                    eprintln!("[Lazy Init] Failed to spawn node {:?}: {}", coord, e);
                } else {
                    println!("[Lazy Init] Successfully triggered spawn for {:?}", coord);
                }
            }
        });
    } else {
        drop(sm);
    }

    // Create SetNodeTarget event
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let event = GameEvent::SetNodeTarget {
        node_coord: req.node_coord,
        target: Some(AttackTarget::Coordinate(req.target_coord)),
        timestamp,
    };

    let request = GameEventRequest { event };

    match state.raft.client_write(request).await {
        Ok(_) => {
            let response = CommandResponse {
                success: true,
                message: format!("Node {:?} now attacking {:?}", req.node_coord, req.target_coord),
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            let response = CommandResponse {
                success: false,
                message: format!("Failed to set attack: {}", e),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
        }
    }
}

/// Handle stop attack command
async fn handle_stop_attack(
    State(state): State<ApiState>,
    Json(req): Json<StopAttackRequest>,
) -> impl IntoResponse {
    // Check if this node is the leader
    let metrics = state.raft.metrics().borrow().clone();
    if metrics.current_leader != Some(metrics.id) {
        let response = CommandResponse {
            success: false,
            message: format!("Not the leader. Current leader: {:?}", metrics.current_leader),
        };
        return (StatusCode::SERVICE_UNAVAILABLE, Json(response));
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let event = GameEvent::SetNodeTarget {
        node_coord: req.node_coord,
        target: None,
        timestamp,
    };

    let request = GameEventRequest { event };

    match state.raft.client_write(request).await {
        Ok(_) => {
            let response = CommandResponse {
                success: true,
                message: format!("Node {:?} stopped attacking", req.node_coord),
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            let response = CommandResponse {
                success: false,
                message: format!("Failed to stop attack: {}", e),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
        }
    }
}

/// Get current game state
async fn handle_get_game_state(State(state): State<ApiState>) -> impl IntoResponse {
    let storage = state.storage.read().await;
    let sm_arc = storage.state_machine();
    drop(storage);
    let sm = sm_arc.read().await;

    let players: Vec<PlayerInfo> = sm
        .game_state
        .players
        .values()
        .map(|p| {
            let node_count = sm
                .game_state
                .nodes
                .values()
                .filter(|n| n.owner_id == p.player_id)
                .count();

            PlayerInfo {
                player_id: p.player_id,
                name: p.name.clone(),
                capital_coord: p.capital_coord,
                alive: p.alive,
                node_count,
            }
        })
        .collect();

    let nodes: Vec<NodeInfo> = sm
        .game_state
        .nodes
        .values()
        .map(|n| {
            // Get metrics for this node if available
            let metrics = sm.game_state.node_metrics.get(&n.coord);

            NodeInfo {
                coord: n.coord,
                owner_id: n.owner_id,
                current_target: n.current_target,
                bandwidth_in: metrics.map(|m| m.bandwidth_in),
                packet_loss: metrics.map(|m| m.packet_loss),
            }
        })
        .collect();

    let total_events = sm.events.len();

    drop(sm);

    let response = GameStateResponse {
        players,
        nodes,
        total_events,
    };

    (StatusCode::OK, Json(response))
}

/// Handle WebSocket upgrade for attack connections
async fn handle_attack(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| handle_attack_websocket_axum(socket))
}

/// Handle WebSocket attack connection (Axum-specific)
async fn handle_attack_websocket_axum(socket: axum::extract::ws::WebSocket) {
    use axum::extract::ws::Message;
    use futures_util::StreamExt;

    println!("[Network] Incoming attack WebSocket connection");

    let (mut sender, _receiver) = socket.split();
    let flood_data = vec![0u8; 1024]; // 1KB per message

    loop {
        // Send data as fast as possible - true flooding
        match sender.send(Message::Binary(flood_data.clone())).await {
            Ok(_) => {
                // Data sent successfully, continue flooding
            }
            Err(_) => {
                break;
            }
        }
    }

    println!("[Network] Attack WebSocket connection closed");
}

/// Helper function to spawn a node on master
async fn spawn_node_on_master(
    master_url: &str,
    game_id: &str,
    q: i32,
    r: i32,
    is_capital: bool,
) -> Result<()> {
    #[derive(Serialize)]
    struct SpawnSingleNodeRequest {
        game_id: String,
        is_capital: bool,
        q: i32,
        r: i32,
    }

    let client = reqwest::Client::new();
    let url = format!("{}/spawn_single_node", master_url);
    let body = SpawnSingleNodeRequest {
        game_id: game_id.to_string(),
        is_capital,
        q,
        r,
    };

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("Master returned error: {}", response.status())
    }
}

/// Start the HTTP API server
pub async fn start_api_server(
    raft: Arc<Raft<GameRaftTypeConfig>>,
    storage: Arc<tokio::sync::RwLock<crate::raft::storage::MemStorage>>,
    addr: String,
    master_url: String,
    game_id: String,
) -> Result<()> {
    let state = ApiState { raft, storage, master_url, game_id };
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("HTTP API server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_serialization() {
        let response = SubmitEventResponse {
            success: true,
            message: "Event committed".to_string(),
            log_index: Some(42),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"log_index\":42"));
    }

    #[test]
    fn test_status_response() {
        let status = StatusResponse {
            node_id: 1,
            is_leader: true,
            current_leader: Some(1),
            current_term: 5,
            event_count: 100,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"node_id\":1"));
        assert!(json.contains("\"is_leader\":true"));
    }
}
