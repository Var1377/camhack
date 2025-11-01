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
) -> Result<Option<PeerInfo>> {
    let master_url = env::var("MASTER_URL")
        .context("MASTER_URL environment variable not set")?;

    let client = reqwest::Client::new();

    // Register ourselves with the master
    println!("Registering with master at {}", master_url);
    let register_req = RegisterWorkerRequest {
        worker_id: worker_id.clone(),
        task_arn,
        ip: my_ip,
        port: RAFT_PORT,
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

    // Get a peer to join (if any exist)
    println!("Requesting peer from master...");
    let peer_response: GetPeerResponse = client
        .get(format!("{}/get_peer", master_url))
        .send()
        .await
        .context("Failed to get peer from master")?
        .json()
        .await
        .context("Failed to parse peer response")?;

    match (peer_response.peer_ip, peer_response.peer_port) {
        (Some(ip), Some(port)) => {
            println!("Got peer from master: {}:{}", ip, port);
            Ok(Some(PeerInfo { ip, port }))
        }
        _ => {
            println!("No peers available - will bootstrap new cluster");
            Ok(None)
        }
    }
}
