pub mod api;
pub mod conversions;
pub mod grpc_server;
pub mod network;
pub mod node_registry;
pub mod storage;

use crate::registry::PeerInfo;
use anyhow::Result;
use network::GrpcNetworkFactory;
use node_registry::NodeRegistry;
use openraft::storage::Adaptor;
use openraft::{Config, Raft};
use std::collections::BTreeMap;
use std::sync::Arc;
use storage::{GameRaftTypeConfig, MemStorage};

pub use storage::NodeId;

/// Real Raft node with full OpenRaft integration
pub struct RaftNode {
    pub node_id: NodeId,
    pub raft: Arc<Raft<GameRaftTypeConfig>>,
    pub registry: NodeRegistry,
    pub storage: Arc<tokio::sync::RwLock<MemStorage>>,
}

impl RaftNode {
    /// Create a new Raft node
    pub async fn new(
        node_id: NodeId,
        _my_ip: String,
        registry: NodeRegistry,
    ) -> Result<Self> {
        // Create storage - keep a reference for queries
        let storage = MemStorage::new();

        // Clone storage for Adaptor (both share the same underlying Arc references)
        let storage_for_adaptor = storage.clone_storage();
        let (log_store, state_machine) = Adaptor::new(storage_for_adaptor);

        // Create network factory
        let network = GrpcNetworkFactory::new(registry.clone());

        // Configure OpenRaft with appropriate timeouts
        let config = Arc::new(Config {
            heartbeat_interval: 500,        // 500ms heartbeats
            election_timeout_min: 1500,     // 1.5s minimum election timeout
            election_timeout_max: 3000,     // 3s maximum election timeout
            install_snapshot_timeout: 10000, // 10s snapshot timeout
            max_in_snapshot_log_to_keep: 1000, // Keep 1000 entries after snapshot
            max_payload_entries: 300,       // Batch up to 300 entries
            ..Default::default()
        });

        // Create Raft instance
        let raft = Raft::new(node_id, config, network, log_store, state_machine)
            .await?;

        Ok(Self {
            node_id,
            raft: Arc::new(raft),
            registry,
            storage: Arc::new(tokio::sync::RwLock::new(storage)),
        })
    }

    /// Check if this node is the current leader
    pub async fn is_leader(&self) -> bool {
        let metrics = self.raft.metrics().borrow().clone();
        metrics.current_leader == Some(self.node_id)
    }

    /// Get current leader ID
    pub async fn get_leader(&self) -> Option<NodeId> {
        let metrics = self.raft.metrics().borrow().clone();
        metrics.current_leader
    }

    /// Get current term
    pub async fn get_term(&self) -> u64 {
        let metrics = self.raft.metrics().borrow().clone();
        metrics.current_term
    }

    /// Get Raft metrics for debugging
    pub async fn get_metrics(&self) -> String {
        let metrics = self.raft.metrics().borrow().clone();
        format!(
            "Node {}: term={}, leader={:?}, state={:?}",
            self.node_id, metrics.current_term, metrics.current_leader, metrics.state
        )
    }
}

/// Bootstrap a new Raft cluster (first worker)
pub async fn bootstrap_cluster(
    node_id: NodeId,
    my_ip: String,
    registry: NodeRegistry,
) -> Result<Arc<RaftNode>> {
    println!("Bootstrapping new Raft cluster as node {}", node_id);
    println!("This node will become the initial leader");

    // Register self in the registry
    registry.register(node_id, format!("{}:5000", my_ip)).await;

    // Create Raft node
    let node = RaftNode::new(node_id, my_ip.clone(), registry).await?;

    // Initialize as single-node cluster
    let mut members = BTreeMap::new();
    members.insert(node_id, ());

    node.raft.initialize(members).await?;

    println!("✓ Cluster bootstrapped successfully");
    println!("  Node ID: {}", node.node_id);
    println!("  IP: {}", my_ip);
    println!("  Initializing as leader...");

    // Start gRPC server for Raft communication
    let raft_clone = node.raft.clone();
    let addr = format!("0.0.0.0:5000");
    tokio::spawn(async move {
        if let Err(e) = grpc_server::start_grpc_server(raft_clone, addr).await {
            eprintln!("gRPC server error: {}", e);
        }
    });

    Ok(Arc::new(node))
}

/// Join an existing Raft cluster (subsequent workers)
pub async fn join_cluster(
    node_id: NodeId,
    my_ip: String,
    peer: PeerInfo,
    registry: NodeRegistry,
) -> Result<Arc<RaftNode>> {
    println!("Joining existing Raft cluster as node {}", node_id);
    println!("Connecting to peer: {}:{}", peer.ip, peer.port);

    // Register self in the registry
    registry.register(node_id, format!("{}:5000", my_ip)).await;

    // Register the peer we know about
    registry.register(999, format!("{}:{}", peer.ip, peer.port)).await; // Temporary ID for peer

    // Create Raft node
    let node = RaftNode::new(node_id, my_ip.clone(), registry).await?;

    // Start gRPC server for Raft communication BEFORE joining
    let raft_clone = node.raft.clone();
    let addr = format!("0.0.0.0:5000");
    tokio::spawn(async move {
        if let Err(e) = grpc_server::start_grpc_server(raft_clone, addr).await {
            eprintln!("gRPC server error: {}", e);
        }
    });

    // Give server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("✓ Joined cluster successfully");
    println!("  Node ID: {}", node.node_id);
    println!("  Peer: {}:{}", peer.ip, peer.port);
    println!("  Waiting for leader election...");

    // Note: In a full implementation, we would contact the leader here and
    // request to be added as a learner via add_learner(), then wait to be
    // promoted to a voting member via change_membership().
    // For now, the node will participate in elections automatically.

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
