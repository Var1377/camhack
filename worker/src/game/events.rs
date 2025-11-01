use serde::{Deserialize, Serialize};

/// Axial coordinates for triangular grid
/// Each node has 6 neighbors at: (q±1, r), (q, r±1), (q±1, r∓1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeCoord {
    pub q: i32,
    pub r: i32,
}

/// Type of node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Player's capital - worker node on grid with larger EC2 instance
    Capital,
    /// Regular captured node - worker node on grid with standard EC2 instance
    Regular,
    /// Client - player's laptop (not on grid, represents the player entity)
    Client,
}

/// Attack target - can attack either a grid coordinate or a player's client
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackTarget {
    /// Attack a node at a specific coordinate on the grid
    Coordinate(NodeCoord),
    /// Attack a player's client (their laptop)
    Player(u64),  // player_id
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
        is_client: bool,  // true if client (cannot attack), false if worker
        timestamp: u64,
    },
    /// Node switches its attack target (or None to stop attacking)
    SetNodeTarget {
        node_coord: NodeCoord,
        target: Option<AttackTarget>,  // None = stop attacking
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
    /// Node initialization started (lazy init triggered)
    NodeInitializationStarted {
        node_coord: NodeCoord,
        owner_id: u64,  // 0 = neutral/unowned
        timestamp: u64,
    },
    /// Node initialization complete (EC2 task is ready)
    NodeInitializationComplete {
        node_coord: NodeCoord,
        node_ip: String,
        timestamp: u64,
    },
}
