use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use aws_sdk_ecs::Client as EcsClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize)]
struct WorkerInfo {
    task_arn: String,
    ip: String,
    port: u16,
    game_id: String,
}

#[derive(Clone, Serialize)]
struct GameCluster {
    game_id: String,
    workers: HashMap<String, WorkerInfo>, // worker_id -> WorkerInfo
    #[serde(skip_serializing)]
    created_at: std::time::SystemTime,
}

#[derive(Clone)]
struct AppState {
    ecs_client: EcsClient,
    cluster_name: String,
    task_definition: String,
    capital_task_definition: String,  // 2x CPU/memory for capitals
    subnet_id: String,
    security_group_id: String,
    games: Arc<RwLock<HashMap<String, GameCluster>>>, // game_id -> GameCluster
    self_task_arn: Option<String>,
}

#[derive(Deserialize)]
struct SpawnQuery {
    count: Option<u32>,
    game_id: Option<String>,
    is_capital: Option<bool>,  // Spawn capital nodes with 2x resources
}

#[derive(Serialize)]
struct SpawnResponse {
    message: String,
    spawned_count: usize,
    task_arns: Vec<String>,
}

#[derive(Serialize)]
struct KillResponse {
    message: String,
    killed_count: usize,
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    active_workers: usize,
    worker_tasks: Vec<String>,
}

#[derive(Deserialize)]
struct RegisterWorkerRequest {
    worker_id: String,
    task_arn: String,
    ip: String,
    port: u16,
    game_id: String,
}

#[derive(Serialize)]
struct RegisterWorkerResponse {
    message: String,
}

#[derive(Deserialize)]
struct GetPeerQuery {
    game_id: String,
}

#[derive(Serialize)]
struct GetPeerResponse {
    peer_ip: Option<String>,
    peer_port: Option<u16>,
}

#[derive(Serialize)]
struct GameInfo {
    game_id: String,
    worker_count: usize,
    created_at_secs: u64,
}

#[derive(Serialize)]
struct GetGamesResponse {
    games: Vec<GameInfo>,
}

#[derive(Deserialize)]
struct SpawnSingleNodeRequest {
    game_id: String,
    is_capital: bool,
    q: i32,  // Node coordinate q
    r: i32,  // Node coordinate r
}

#[derive(Serialize)]
struct SpawnSingleNodeResponse {
    message: String,
    task_arn: Option<String>,
    coord: String,
}

#[tokio::main]
async fn main() {
    println!("Master node starting...");

    // Load AWS configuration
    let config = aws_config::load_from_env().await;
    let ecs_client = EcsClient::new(&config);

    // Get configuration from environment
    let cluster_name = std::env::var("CLUSTER_NAME")
        .unwrap_or_else(|_| "udp-test-cluster".to_string());
    let task_definition = std::env::var("WORKER_TASK_DEFINITION")
        .unwrap_or_else(|_| "worker".to_string());
    let capital_task_definition = std::env::var("CAPITAL_TASK_DEFINITION")
        .unwrap_or_else(|_| "worker-capital".to_string());
    let subnet_id = std::env::var("SUBNET_ID")
        .expect("SUBNET_ID environment variable required");
    let security_group_id = std::env::var("SECURITY_GROUP_ID")
        .expect("SECURITY_GROUP_ID environment variable required");

    // Try to get our own task ARN (for self-kill)
    let self_task_arn = std::env::var("SELF_TASK_ARN").ok();

    let state = AppState {
        ecs_client,
        cluster_name,
        task_definition,
        capital_task_definition,
        subnet_id,
        security_group_id,
        games: Arc::new(RwLock::new(HashMap::new())),
        self_task_arn,
    };

    // Build HTTP router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/spawn_workers", post(spawn_workers))
        .route("/spawn_single_node", post(spawn_single_node))
        .route("/kill_workers", post(kill_workers))
        .route("/kill", post(kill_self))
        .route("/status", get(status))
        .route("/register_worker", post(register_worker))
        .route("/get_peer", get(get_peer))
        .route("/games", get(get_games))
        .with_state(state);

    // Start HTTP server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("Invalid PORT");

    let addr = format!("0.0.0.0:{}", port);
    println!("Master node listening on {}", addr);
    println!("Endpoints:");
    println!("  GET  /                - Health check");
    println!("  GET  /status          - Show active workers");
    println!("  GET  /games           - List all available games");
    println!("  POST /spawn_workers?count=N&game_id=X - Spawn N workers for game X");
    println!("  POST /kill_workers    - Kill all workers");
    println!("  POST /kill            - Kill master (self)");
    println!("  POST /register_worker - Register worker with master");
    println!("  GET  /get_peer?game_id=X - Get a peer for joining cluster");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "Master node is alive"
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let games = state.games.read().await;

    // Count total workers across all games
    let total_workers: usize = games.values().map(|g| g.workers.len()).sum();

    // Collect all worker IDs across all games
    let all_worker_ids: Vec<String> = games.values()
        .flat_map(|g| g.workers.keys().cloned())
        .collect();

    let response = StatusResponse {
        status: "running".to_string(),
        active_workers: total_workers,
        worker_tasks: all_worker_ids,
    };

    Json(response)
}

async fn spawn_workers(
    Query(params): Query<SpawnQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let count = params.count.unwrap_or(1);
    let game_id = params.game_id.unwrap_or_else(|| "default-game".to_string());
    let is_capital = params.is_capital.unwrap_or(false);

    println!(
        "Spawning {} {} workers for game {}...",
        count,
        if is_capital { "capital" } else { "regular" },
        game_id
    );

    let mut spawned_arns = Vec::new();

    // Select task definition based on whether it's a capital
    let task_def = if is_capital {
        &state.capital_task_definition
    } else {
        &state.task_definition
    };

    // Build task overrides to set GAME_ID environment variable
    let mut task_override = aws_sdk_ecs::types::TaskOverride::builder();

    let container_override = aws_sdk_ecs::types::ContainerOverride::builder()
        .name(if is_capital { "udp-node-capital" } else { "udp-node" })
        .environment(
            aws_sdk_ecs::types::KeyValuePair::builder()
                .name("GAME_ID")
                .value(&game_id)
                .build()
        )
        .build();

    task_override = task_override.container_overrides(container_override);

    // Spawn workers using ECS run_task
    match state
        .ecs_client
        .run_task()
        .cluster(&state.cluster_name)
        .task_definition(task_def)
        .count(count as i32)
        .launch_type(aws_sdk_ecs::types::LaunchType::Fargate)
        .network_configuration(
            aws_sdk_ecs::types::NetworkConfiguration::builder()
                .awsvpc_configuration(
                    aws_sdk_ecs::types::AwsVpcConfiguration::builder()
                        .subnets(&state.subnet_id)
                        .security_groups(&state.security_group_id)
                        .assign_public_ip(aws_sdk_ecs::types::AssignPublicIp::Enabled)
                        .build()
                        .unwrap(),
                )
                .build(),
        )
        .overrides(task_override.build())
        .send()
        .await
    {
        Ok(response) => {
            if let Some(tasks) = response.tasks {
                for task in tasks {
                    if let Some(task_arn) = task.task_arn {
                        println!("Spawned worker: {}", task_arn);
                        spawned_arns.push(task_arn.clone());
                    }
                }
            }

            // Note: Workers will register themselves with /register_worker after they start
            // We just track that we spawned them via ECS

            (
                StatusCode::OK,
                Json(SpawnResponse {
                    message: format!("Successfully spawned {} workers for game {}", spawned_arns.len(), game_id),
                    spawned_count: spawned_arns.len(),
                    task_arns: spawned_arns,
                }),
            )
        }
        Err(e) => {
            eprintln!("Failed to spawn workers: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SpawnResponse {
                    message: format!("Failed to spawn workers: {}", e),
                    spawned_count: 0,
                    task_arns: vec![],
                }),
            )
        }
    }
}

async fn spawn_single_node(
    State(state): State<AppState>,
    Json(payload): Json<SpawnSingleNodeRequest>,
) -> impl IntoResponse {
    let coord_str = format!("({}, {})", payload.q, payload.r);
    println!(
        "Spawning single {} node at {} for game {}...",
        if payload.is_capital { "capital" } else { "regular" },
        coord_str,
        payload.game_id
    );

    // Select task definition based on whether it's a capital
    let task_def = if payload.is_capital {
        &state.capital_task_definition
    } else {
        &state.task_definition
    };

    // Build task overrides to set GAME_ID and NODE_COORD environment variables
    let container_override = aws_sdk_ecs::types::ContainerOverride::builder()
        .name(if payload.is_capital { "udp-node-capital" } else { "udp-node" })
        .environment(
            aws_sdk_ecs::types::KeyValuePair::builder()
                .name("GAME_ID")
                .value(&payload.game_id)
                .build()
        )
        .environment(
            aws_sdk_ecs::types::KeyValuePair::builder()
                .name("NODE_COORD_Q")
                .value(payload.q.to_string())
                .build()
        )
        .environment(
            aws_sdk_ecs::types::KeyValuePair::builder()
                .name("NODE_COORD_R")
                .value(payload.r.to_string())
                .build()
        )
        .build();

    let task_override = aws_sdk_ecs::types::TaskOverride::builder()
        .container_overrides(container_override)
        .build();

    // Spawn single task
    match state
        .ecs_client
        .run_task()
        .cluster(&state.cluster_name)
        .task_definition(task_def)
        .count(1)
        .launch_type(aws_sdk_ecs::types::LaunchType::Fargate)
        .network_configuration(
            aws_sdk_ecs::types::NetworkConfiguration::builder()
                .awsvpc_configuration(
                    aws_sdk_ecs::types::AwsVpcConfiguration::builder()
                        .subnets(&state.subnet_id)
                        .security_groups(&state.security_group_id)
                        .assign_public_ip(aws_sdk_ecs::types::AssignPublicIp::Enabled)
                        .build()
                        .unwrap(),
                )
                .build(),
        )
        .overrides(task_override)
        .send()
        .await
    {
        Ok(response) => {
            let task_arn = response
                .tasks
                .and_then(|tasks| tasks.first().and_then(|task| task.task_arn.clone()));

            if let Some(ref arn) = task_arn {
                println!("Spawned single node: {}", arn);
            }

            (
                StatusCode::OK,
                Json(SpawnSingleNodeResponse {
                    message: format!(
                        "Successfully spawned {} node at {} for game {}",
                        if payload.is_capital { "capital" } else { "regular" },
                        coord_str,
                        payload.game_id
                    ),
                    task_arn,
                    coord: coord_str,
                }),
            )
        }
        Err(e) => {
            eprintln!("Failed to spawn single node: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SpawnSingleNodeResponse {
                    message: format!("Failed to spawn node: {}", e),
                    task_arn: None,
                    coord: coord_str,
                }),
            )
        }
    }
}

async fn kill_workers(State(state): State<AppState>) -> impl IntoResponse {
    let games = state.games.read().await.clone();

    // Collect all workers across all games
    let all_workers: Vec<(&String, &WorkerInfo)> = games.values()
        .flat_map(|game| game.workers.iter())
        .collect();

    println!("Killing {} workers across {} games...", all_workers.len(), games.len());

    let mut killed_count = 0;

    for (worker_id, worker_info) in &all_workers {
        match state
            .ecs_client
            .stop_task()
            .cluster(&state.cluster_name)
            .task(&worker_info.task_arn)
            .send()
            .await
        {
            Ok(_) => {
                println!("Killed worker {}: {}", worker_id, worker_info.task_arn);
                killed_count += 1;
            }
            Err(e) => {
                eprintln!("Failed to kill worker {} ({}): {}", worker_id, worker_info.task_arn, e);
            }
        }
    }

    // Clear all games
    state.games.write().await.clear();

    (
        StatusCode::OK,
        Json(KillResponse {
            message: format!("Killed {} workers", killed_count),
            killed_count,
        }),
    )
}

async fn kill_self(State(state): State<AppState>) -> impl IntoResponse {
    println!("Master received kill command, terminating self...");

    if let Some(task_arn) = &state.self_task_arn {
        // Stop our own task
        match state
            .ecs_client
            .stop_task()
            .cluster(&state.cluster_name)
            .task(task_arn)
            .send()
            .await
        {
            Ok(_) => {
                println!("Successfully initiated self-termination");
                (StatusCode::OK, "Master terminating...")
            }
            Err(e) => {
                eprintln!("Failed to stop self: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to terminate master",
                )
            }
        }
    } else {
        eprintln!("SELF_TASK_ARN not set, cannot self-terminate via ECS");

        // Fallback: exit the process (container will stop)
        std::process::exit(0);
    }
}

async fn register_worker(
    State(state): State<AppState>,
    Json(payload): Json<RegisterWorkerRequest>,
) -> impl IntoResponse {
    println!(
        "Registering worker: {} at {}:{} for game: {}",
        payload.worker_id, payload.ip, payload.port, payload.game_id
    );

    let worker_info = WorkerInfo {
        task_arn: payload.task_arn,
        ip: payload.ip,
        port: payload.port,
        game_id: payload.game_id.clone(),
    };

    let mut games = state.games.write().await;

    // Get or create the game cluster
    let game_cluster = games.entry(payload.game_id.clone()).or_insert_with(|| {
        println!("Creating new game cluster: {}", payload.game_id);
        GameCluster {
            game_id: payload.game_id.clone(),
            workers: HashMap::new(),
            created_at: std::time::SystemTime::now(),
        }
    });

    // Add worker to the game cluster
    game_cluster.workers.insert(payload.worker_id.clone(), worker_info);

    println!(
        "Worker {} registered to game {}. Workers in this game: {}",
        payload.worker_id,
        payload.game_id,
        game_cluster.workers.len()
    );

    (
        StatusCode::OK,
        Json(RegisterWorkerResponse {
            message: format!("Worker {} registered successfully to game {}", payload.worker_id, payload.game_id),
        }),
    )
}

async fn get_peer(
    Query(params): Query<GetPeerQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let games = state.games.read().await;

    // Look for the requested game
    if let Some(game_cluster) = games.get(&params.game_id) {
        if game_cluster.workers.is_empty() {
            println!("No peers available for game {} - this will be the first worker", params.game_id);
            return Json(GetPeerResponse {
                peer_ip: None,
                peer_port: None,
            });
        }

        // Return a peer from this game (just pick the first one for simplicity)
        if let Some((_worker_id, worker_info)) = game_cluster.workers.iter().next() {
            println!(
                "Returning peer for game {}: {}:{}",
                params.game_id, worker_info.ip, worker_info.port
            );
            return Json(GetPeerResponse {
                peer_ip: Some(worker_info.ip.clone()),
                peer_port: Some(worker_info.port),
            });
        }
    }

    // Game doesn't exist yet - this will be the first worker
    println!("No game cluster found for {} - this will be the first worker", params.game_id);
    Json(GetPeerResponse {
        peer_ip: None,
        peer_port: None,
    })
}

async fn get_games(State(state): State<AppState>) -> impl IntoResponse {
    let games = state.games.read().await;

    let game_infos: Vec<GameInfo> = games.values()
        .map(|game_cluster| {
            // Convert SystemTime to seconds since UNIX_EPOCH
            let created_at_secs = game_cluster.created_at
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            GameInfo {
                game_id: game_cluster.game_id.clone(),
                worker_count: game_cluster.workers.len(),
                created_at_secs,
            }
        })
        .collect();

    println!("Returning {} active games", game_infos.len());

    Json(GetGamesResponse {
        games: game_infos,
    })
}
