pub mod storage;

use crate::registry::PeerInfo;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

pub use storage::{GameRaftTypeConfig, MemStorage, NodeId};

/// Simplified Raft node wrapper
/// TODO: Integrate full OpenRaft once basic architecture is working
pub struct RaftNode {
    pub node_id: NodeId,
    pub my_ip: String,
    pub storage: Arc<RwLock<MemStorage>>,
    pub is_leader: Arc<RwLock<bool>>,
}

impl RaftNode {
    pub fn new(node_id: NodeId, my_ip: String) -> Self {
        Self {
            node_id,
            my_ip,
            storage: Arc::new(RwLock::new(MemStorage::new())),
            is_leader: Arc::new(RwLock::new(false)),
        }
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        *self.is_leader.read().await
    }
}

/// Bootstrap a new Raft cluster (first worker)
pub async fn bootstrap_cluster(node_id: NodeId, my_ip: String) -> Result<Arc<RaftNode>> {
    println!("Bootstrapping new Raft cluster as node {}", node_id);
    println!("This node will become the initial leader");

    let node = RaftNode::new(node_id, my_ip);

    // First node becomes leader automatically
    *node.is_leader.write().await = true;

    println!("✓ Cluster bootstrapped successfully");
    println!("  Node ID: {}", node.node_id);
    println!("  Is Leader: true");

    Ok(Arc::new(node))
}

/// Join an existing Raft cluster (subsequent workers)
pub async fn join_cluster(node_id: NodeId, my_ip: String, peer: PeerInfo) -> Result<Arc<RaftNode>> {
    println!("Joining existing Raft cluster as node {}", node_id);
    println!("Connecting to peer: {}:{}", peer.ip, peer.port);

    let node = RaftNode::new(node_id, my_ip);

    // TODO: Implement actual cluster join via OpenRaft
    // For now, just create the node as a follower
    *node.is_leader.write().await = false;

    println!("✓ Joined cluster successfully");
    println!("  Node ID: {}", node.node_id);
    println!("  Peer: {}:{}", peer.ip, peer.port);
    println!("  Is Leader: false");

    Ok(Arc::new(node))
}

/// Generate a unique node ID (simplified - in production use better ID generation)
pub fn generate_node_id() -> NodeId {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}
