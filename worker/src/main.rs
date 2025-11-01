mod game;
mod metadata;
mod raft;
mod registry;

use anyhow::Result;
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

    // Step 4: Register with master and get peer
    println!("\n[3/5] Registering with master...");
    let peer = registry::register_and_get_peer(worker_id.clone(), task_arn, my_ip.clone()).await?;

    // Step 5: Initialize Raft node
    println!("\n[4/5] Initializing Raft node...");
    let node_id = raft::generate_node_id();
    let raft_node = if let Some(peer_info) = peer {
        // Join existing cluster
        raft::join_cluster(node_id, my_ip.clone(), peer_info).await?
    } else {
        // Bootstrap new cluster
        raft::bootstrap_cluster(node_id, my_ip.clone()).await?
    };

    println!("\n[5/5] Worker startup complete!");
    println!("\n=== Worker Node Ready ===");
    println!("  Worker ID: {}", worker_id);
    println!("  Node ID: {}", node_id);
    println!("  IP: {}", my_ip);
    println!("  Is Leader: {}", raft_node.is_leader().await);
    println!("========================\n");

    // Main loop - keep worker alive and periodically show status
    let mut tick_count = 0;
    loop {
        sleep(Duration::from_secs(30)).await;
        tick_count += 1;

        let is_leader = raft_node.is_leader().await;
        let storage = raft_node.storage.read().await;
        let sm = storage.state_machine().read().await;
        let event_count = sm.events.len();
        drop(sm);
        drop(storage);

        println!(
            "[Tick {}] Worker {} | Leader: {} | Events: {}",
            tick_count, worker_id, is_leader, event_count
        );
    }
}
