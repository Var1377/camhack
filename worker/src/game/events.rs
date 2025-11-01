use serde::{Deserialize, Serialize};

/// Game event types - classified by whether they require consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    /// Critical events that go through Raft consensus
    Critical(CriticalEvent),
    /// Ephemeral events that are leader-only (no consensus)
    Ephemeral(EphemeralEvent),
}

/// Critical events that require strong consistency via Raft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CriticalEvent {
    /// Player joins the game
    PlayerJoin {
        player_id: u64,
        name: String,
        timestamp: u64,
    },
    /// Player leaves the game
    PlayerLeave {
        player_id: u64,
        timestamp: u64,
    },
    /// Score update
    ScoreUpdate {
        player_id: u64,
        score: i32,
        timestamp: u64,
    },
    /// Inventory change
    InventoryChange {
        player_id: u64,
        item_id: u64,
        quantity: i32,
        timestamp: u64,
    },
    /// Game state checkpoint
    StateCheckpoint {
        checkpoint_id: u64,
        data: Vec<u8>,
        timestamp: u64,
    },
}

/// Ephemeral events that don't require consensus (leader-only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EphemeralEvent {
    /// Player movement
    PlayerMove {
        player_id: u64,
        x: f32,
        y: f32,
        z: f32,
        timestamp: u64,
    },
    /// Player rotation
    PlayerRotate {
        player_id: u64,
        yaw: f32,
        pitch: f32,
        timestamp: u64,
    },
    /// Projectile spawned
    ProjectileSpawn {
        projectile_id: u64,
        x: f32,
        y: f32,
        z: f32,
        velocity_x: f32,
        velocity_y: f32,
        velocity_z: f32,
        timestamp: u64,
    },
    /// Animation state change
    AnimationState {
        entity_id: u64,
        animation: String,
        timestamp: u64,
    },
    /// Chat message
    ChatMessage {
        player_id: u64,
        message: String,
        timestamp: u64,
    },
}

impl GameEvent {
    /// Check if this event requires Raft consensus
    pub fn requires_consensus(&self) -> bool {
        matches!(self, GameEvent::Critical(_))
    }
}
