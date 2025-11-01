// Library interface for sharing Raft and game logic components
// between worker and client binaries

pub mod game;
pub mod metadata;
pub mod raft;
pub mod registry;

// Re-export commonly used types for convenience
pub use raft::{RaftNode, generate_node_id, bootstrap_cluster, join_cluster};
pub use raft::storage::{MemStorage, GameStateMachine, GameEventRequest};
pub use raft::node_registry::NodeRegistry;
pub use game::{GameState, GameEvent, GameConfig, GameLogic};
