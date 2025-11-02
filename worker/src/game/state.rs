use super::events::{AttackTarget, GameEvent, NodeCoord, NodeType};
use std::collections::HashMap;

/// Player state
#[derive(Debug, Clone)]
pub struct Player {
    pub player_id: u64,
    pub name: String,
    pub capital_coord: NodeCoord,
    pub alive: bool,
    pub join_time: u64,
}

/// Node initialization state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeInitState {
    /// EC2 task is spawning, no IP available yet
    Initializing,
    /// EC2 instance is running and has IP address
    Ready,
}

/// Node on the grid
/// Capacity is determined by actual EC2 instance type, not hardcoded
#[derive(Debug, Clone)]
pub struct Node {
    pub coord: NodeCoord,
    pub owner_id: u64,
    pub node_type: NodeType,
    pub current_target: Option<AttackTarget>,  // What this node is attacking
    pub is_client: bool,  // true if this is a client node
    pub init_state: NodeInitState,  // Whether EC2 is ready
}

/// Metrics for a node at a point in time
#[derive(Debug, Clone)]
pub struct NodeMetrics {
    pub bandwidth_in: u64,
    pub packet_loss: f32,
    pub timestamp: u64,
}

/// Derived attack information
#[derive(Debug, Clone)]
pub struct Attack {
    pub attacker_node: NodeCoord,
    pub attacker_owner: u64,
    pub target_node: NodeCoord,
    pub target_owner: u64,
}

/// Complete game state derived from events
#[derive(Debug, Clone)]
pub struct GameState {
    /// All players
    pub players: HashMap<u64, Player>,
    /// All nodes on the grid (coord -> node)
    pub nodes: HashMap<NodeCoord, Node>,
    /// Latest metrics for each node
    pub node_metrics: HashMap<NodeCoord, NodeMetrics>,
    /// IP addresses of nodes (coord -> IP)
    pub node_ips: HashMap<NodeCoord, String>,
    /// IP addresses of client nodes (player_id -> IP)
    pub client_ips: HashMap<u64, String>,
    /// Last applied log index
    pub last_applied_log_index: u64,
    /// Game is over (only one player remaining)
    pub game_over: bool,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            nodes: HashMap::new(),
            node_metrics: HashMap::new(),
            node_ips: HashMap::new(),
            client_ips: HashMap::new(),
            last_applied_log_index: 0,
            game_over: false,
        }
    }

    /// Process a game event and update state
    pub fn process_event(&mut self, event: GameEvent, log_index: u64) {
        self.last_applied_log_index = log_index;

        match event {
            GameEvent::PlayerJoin {
                player_id,
                name,
                capital_coord,
                node_ip,
                is_client,
                timestamp,
            } => {
                // Create player
                let player = Player {
                    player_id,
                    name,
                    capital_coord,
                    alive: true,
                    join_time: timestamp,
                };
                self.players.insert(player_id, player);

                // Create capital node (capacity determined by EC2 instance type)
                let capital = Node {
                    coord: capital_coord,
                    owner_id: player_id,
                    node_type: if is_client { NodeType::Client } else { NodeType::Capital },
                    current_target: None,
                    is_client,
                    init_state: NodeInitState::Ready,  // Has EC2 already
                };
                self.nodes.insert(capital_coord, capital);

                // Store IP address
                self.node_ips.insert(capital_coord, node_ip.clone());

                // If this is a client node, also store in client_ips map
                if is_client {
                    self.client_ips.insert(player_id, node_ip);
                }
            }

            GameEvent::SetNodeTarget {
                node_coord,
                target,
                ..
            } => {
                if let Some(node) = self.nodes.get_mut(&node_coord) {
                    node.current_target = target.clone();
                }
            }

            GameEvent::NodeCaptured {
                node_coord,
                new_owner_id,
                ..
            } => {
                if let Some(node) = self.nodes.get_mut(&node_coord) {
                    let old_owner_id = node.owner_id;
                    node.owner_id = new_owner_id;
                    node.current_target = None;  // Stop attacking when captured

                    // If this was a capital, the old owner loses
                    if node.node_type == NodeType::Capital {
                        if let Some(old_owner) = self.players.get_mut(&old_owner_id) {
                            old_owner.alive = false;
                        }

                        // Convert capital to regular node for new owner
                        // (EC2 instance stays the same size - still has capital-level resources)
                        node.node_type = NodeType::Regular;

                        // Check if only one player remains (game over)
                        let alive_count = self.players.values().filter(|p| p.alive).count();
                        if alive_count <= 1 {
                            self.game_over = true;
                        }
                    }
                }
            }

            GameEvent::NodeMetricsReport {
                node_coord,
                bandwidth_in,
                packet_loss,
                timestamp,
            } => {
                let metrics = NodeMetrics {
                    bandwidth_in,
                    packet_loss,
                    timestamp,
                };
                self.node_metrics.insert(node_coord, metrics);
            }

            GameEvent::NodeInitializationStarted {
                node_coord,
                owner_id,
                ..
            } => {
                // Only insert if node doesn't already exist (deduplication)
                if !self.nodes.contains_key(&node_coord) {
                    // Create placeholder node in Initializing state
                    let node = Node {
                        coord: node_coord,
                        owner_id,
                        node_type: NodeType::Regular,  // Lazily initialized nodes are regular
                        current_target: None,
                        is_client: false,
                        init_state: NodeInitState::Initializing,
                    };
                    self.nodes.insert(node_coord, node);
                }
            }

            GameEvent::NodeInitializationComplete {
                node_coord,
                node_ip,
                ..
            } => {
                // Update node to Ready state and store IP
                if let Some(node) = self.nodes.get_mut(&node_coord) {
                    node.init_state = NodeInitState::Ready;
                }
                self.node_ips.insert(node_coord, node_ip);
            }
        }
    }

    /// Get all active attacks
    pub fn get_active_attacks(&self) -> Vec<Attack> {
        let mut attacks = Vec::new();

        for node in self.nodes.values() {
            if let Some(AttackTarget::Coordinate(target_coord)) = node.current_target {
                if let Some(target_node) = self.nodes.get(&target_coord) {
                    attacks.push(Attack {
                        attacker_node: node.coord,
                        attacker_owner: node.owner_id,
                        target_node: target_coord,
                        target_owner: target_node.owner_id,
                    });
                }
            }
        }

        attacks
    }

    /// Get all nodes owned by a player
    pub fn get_player_nodes(&self, player_id: u64) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|node| node.owner_id == player_id)
            .collect()
    }

    // Note: Bandwidth is now measured via actual UDP/ACK metrics,
    // not estimated from capacity
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_join() {
        let mut state = GameState::new();
        let event = GameEvent::PlayerJoin {
            player_id: 1,
            name: "Alice".to_string(),
            capital_coord: NodeCoord::new(0, 0),
            node_ip: "10.0.0.1".to_string(),
            is_client: false,
            timestamp: 1000,
        };

        state.process_event(event, 1);

        assert_eq!(state.players.len(), 1);
        assert_eq!(state.nodes.len(), 1);
        assert!(state.players.get(&1).unwrap().alive);
    }

    #[test]
    fn test_node_capture() {
        let mut state = GameState::new();

        // Add two players
        state.process_event(
            GameEvent::PlayerJoin {
                player_id: 1,
                name: "Alice".to_string(),
                capital_coord: NodeCoord::new(0, 0),
                node_ip: "10.0.0.1".to_string(),
                is_client: false,
                timestamp: 1000,
            },
            1,
        );
        state.process_event(
            GameEvent::PlayerJoin {
                player_id: 2,
                name: "Bob".to_string(),
                capital_coord: NodeCoord::new(1, 0),
                node_ip: "10.0.0.2".to_string(),
                is_client: false,
                timestamp: 1001,
            },
            2,
        );

        // Bob captures Alice's capital
        state.process_event(
            GameEvent::NodeCaptured {
                node_coord: NodeCoord::new(0, 0),
                new_owner_id: 2,
                timestamp: 2000,
            },
            3,
        );

        // Alice should be dead
        assert!(!state.players.get(&1).unwrap().alive);
        // Bob should own the node
        assert_eq!(state.nodes.get(&NodeCoord::new(0, 0)).unwrap().owner_id, 2);
    }
}
