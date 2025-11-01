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
}

#[derive(Clone)]
struct AppState {
    ecs_client: EcsClient,
    cluster_name: String,
    task_definition: String,
    subnet_id: String,
    security_group_id: String,
    worker_tasks: Arc<RwLock<HashMap<String, WorkerInfo>>>, // worker_id -> WorkerInfo
    self_task_arn: Option<String>,
}

#[derive(Deserialize)]
struct SpawnQuery {
    count: Option<u32>,
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
}

#[derive(Serialize)]
struct RegisterWorkerResponse {
    message: String,
}

#[derive(Serialize)]
struct GetPeerResponse {
    peer_ip: Option<String>,
    peer_port: Option<u16>,
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
        subnet_id,
        security_group_id,
        worker_tasks: Arc::new(RwLock::new(HashMap::new())),
        self_task_arn,
    };

    // Build HTTP router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/spawn_workers", post(spawn_workers))
        .route("/kill_workers", post(kill_workers))
        .route("/kill", post(kill_self))
        .route("/status", get(status))
        .route("/register_worker", post(register_worker))
        .route("/get_peer", get(get_peer))
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
    println!("  POST /spawn_workers?count=N - Spawn N workers");
    println!("  POST /kill_workers    - Kill all workers");
    println!("  POST /kill            - Kill master (self)");
    println!("  POST /register_worker - Register worker with master");
    println!("  GET  /get_peer        - Get a peer for joining cluster");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "Master node is alive"
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.worker_tasks.read().await;

    let response = StatusResponse {
        status: "running".to_string(),
        active_workers: workers.len(),
        worker_tasks: workers.keys().cloned().collect(),
    };

    Json(response)
}

async fn spawn_workers(
    Query(params): Query<SpawnQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let count = params.count.unwrap_or(1);

    println!("Spawning {} workers...", count);

    let mut spawned_arns = Vec::new();

    // Spawn workers using ECS run_task
    match state
        .ecs_client
        .run_task()
        .cluster(&state.cluster_name)
        .task_definition(&state.task_definition)
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
                    message: format!("Successfully spawned {} workers", spawned_arns.len()),
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

async fn kill_workers(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.worker_tasks.read().await.clone();

    println!("Killing {} workers...", workers.len());

    let mut killed_count = 0;

    for (worker_id, worker_info) in &workers {
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

    // Clear the worker list
    state.worker_tasks.write().await.clear();

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
        "Registering worker: {} at {}:{}",
        payload.worker_id, payload.ip, payload.port
    );

    let worker_info = WorkerInfo {
        task_arn: payload.task_arn,
        ip: payload.ip,
        port: payload.port,
    };

    let mut workers = state.worker_tasks.write().await;
    workers.insert(payload.worker_id.clone(), worker_info);

    println!("Worker {} registered. Total workers: {}", payload.worker_id, workers.len());

    (
        StatusCode::OK,
        Json(RegisterWorkerResponse {
            message: format!("Worker {} registered successfully", payload.worker_id),
        }),
    )
}

async fn get_peer(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.worker_tasks.read().await;

    if workers.is_empty() {
        println!("No peers available - this will be the first worker");
        return Json(GetPeerResponse {
            peer_ip: None,
            peer_port: None,
        });
    }

    // Return a random peer (just pick the first one for simplicity)
    if let Some((_worker_id, worker_info)) = workers.iter().next() {
        println!("Returning peer: {}:{}", worker_info.ip, worker_info.port);
        Json(GetPeerResponse {
            peer_ip: Some(worker_info.ip.clone()),
            peer_port: Some(worker_info.port),
        })
    } else {
        Json(GetPeerResponse {
            peer_ip: None,
            peer_port: None,
        })
    }
}
