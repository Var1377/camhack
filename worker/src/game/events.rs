use serde::{Deserialize, Serialize};

/// Axial coordinates for triangular grid
/// Each node has 6 neighbors at: (q±1, r), (q, r±1), (q±1, r∓1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeCoord {
    pub q: i32,
    pub r: i32,
}

/// Type of node on the grid
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Player's capital - starts on grid, larger capacity
    Capital,
    /// Regular captured node
    Regular,
}

/// Game events - all go through Raft consensus for CamHack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    /// Player joins the game with local node + capital on grid
    PlayerJoin {
        player_id: u64,
        name: String,
        capital_coord: NodeCoord,
        node_ip: String,  // IP address of the worker/node
        timestamp: u64,
    },
    /// Node switches its attack target (or None to stop attacking)
    SetNodeTarget {
        node_coord: NodeCoord,
        target_coord: Option<NodeCoord>,
        timestamp: u64,
    },
    /// Node is captured after sustained overload
    NodeCaptured {
        node_coord: NodeCoord,
        new_owner_id: u64,
        timestamp: u64,
    },
    /// Node reports its metrics (bandwidth, packet loss)
    NodeMetricsReport {
        node_coord: NodeCoord,
        bandwidth_in: u64,  // bytes/sec
        packet_loss: f32,   // 0.0 to 1.0
        timestamp: u64,
    },
}
