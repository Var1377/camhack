use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;

const RAFT_PORT: u16 = 5000;

#[derive(Debug, Serialize)]
struct RegisterWorkerRequest {
    worker_id: String,
    task_arn: String,
    ip: String,
    port: u16,
    game_id: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RegisterWorkerResponse {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GetPeerResponse {
    peer_ip: Option<String>,
    peer_port: Option<u16>,
}

/// Peer information for joining a Raft cluster
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub ip: String,
    pub port: u16,
}

/// Register this worker with the master and get a peer to join (if any)
pub async fn register_and_get_peer(
    worker_id: String,
    task_arn: String,
    my_ip: String,
    game_id: String,
) -> Result<Option<PeerInfo>> {
    let master_url = env::var("MASTER_URL")
        .context("MASTER_URL environment variable not set")?;

    let client = reqwest::Client::new();

    // Register ourselves with the master
    println!("Registering with master at {} for game {}", master_url, game_id);
    let register_req = RegisterWorkerRequest {
        worker_id: worker_id.clone(),
        task_arn,
        ip: my_ip,
        port: RAFT_PORT,
        game_id: game_id.clone(),
    };

    let response: RegisterWorkerResponse = client
        .post(format!("{}/register_worker", master_url))
        .json(&register_req)
        .send()
        .await
        .context("Failed to register with master")?
        .json()
        .await
        .context("Failed to parse registration response")?;

    println!("Registration response: {}", response.message);

    // Get a peer to join (if any exist) for this specific game
    println!("Requesting peer from master for game {}...", game_id);
    let peer_response: GetPeerResponse = client
        .get(format!("{}/get_peer?game_id={}", master_url, game_id))
        .send()
        .await
        .context("Failed to get peer from master")?
        .json()
        .await
        .context("Failed to parse peer response")?;

    match (peer_response.peer_ip, peer_response.peer_port) {
        (Some(ip), Some(port)) => {
            println!("Got peer from master for game {}: {}:{}", game_id, ip, port);
            Ok(Some(PeerInfo { ip, port }))
        }
        _ => {
            println!("No peers available for game {} - will bootstrap new cluster", game_id);
            Ok(None)
        }
    }
}
