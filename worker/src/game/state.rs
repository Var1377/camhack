use super::events::{GameEvent, NodeCoord, NodeType};
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

/// Node on the grid
#[derive(Debug, Clone)]
pub struct Node {
    pub coord: NodeCoord,
    pub owner_id: u64,
    pub node_type: NodeType,
    pub capacity: u64,  // bytes/sec bandwidth capacity
    pub current_target: Option<NodeCoord>,  // What this node is attacking
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
    /// Last applied log index
    pub last_applied_log_index: u64,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            nodes: HashMap::new(),
            node_metrics: HashMap::new(),
            node_ips: HashMap::new(),
            last_applied_log_index: 0,
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

                // Create capital node
                let capital = Node {
                    coord: capital_coord,
                    owner_id: player_id,
                    node_type: NodeType::Capital,
                    capacity: 10_000_000,  // 10 MB/s for capital (larger EC2)
                    current_target: None,
                };
                self.nodes.insert(capital_coord, capital);

                // Store IP address
                self.node_ips.insert(capital_coord, node_ip);
            }

            GameEvent::SetNodeTarget {
                node_coord,
                target_coord,
                ..
            } => {
                if let Some(node) = self.nodes.get_mut(&node_coord) {
                    node.current_target = target_coord;
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
                        node.node_type = NodeType::Regular;
                        node.capacity = 1_000_000;  // 1 MB/s for regular node
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
        }
    }

    /// Get all active attacks
    pub fn get_active_attacks(&self) -> Vec<Attack> {
        let mut attacks = Vec::new();

        for node in self.nodes.values() {
            if let Some(target_coord) = node.current_target {
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

    /// Check if a player can attack a target (must own adjacent node)
    pub fn can_attack(&self, player_id: u64, target: NodeCoord) -> bool {
        // Check if target exists
        if !self.nodes.contains_key(&target) {
            return false;
        }

        // Check if player owns any adjacent node
        for neighbor_coord in target.neighbors() {
            if let Some(neighbor) = self.nodes.get(&neighbor_coord) {
                if neighbor.owner_id == player_id {
                    return true;
                }
            }
        }

        false
    }

    /// Get all nodes owned by a player
    pub fn get_player_nodes(&self, player_id: u64) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|node| node.owner_id == player_id)
            .collect()
    }

    /// Get bandwidth being sent to a target node (sum of all attackers)
    pub fn get_incoming_bandwidth(&self, target: NodeCoord) -> HashMap<NodeCoord, u64> {
        let mut bandwidth_per_attacker = HashMap::new();

        // Find all nodes attacking this target
        for node in self.nodes.values() {
            if node.current_target == Some(target) {
                // Estimate bandwidth: use node's capacity as max output
                bandwidth_per_attacker.insert(node.coord, node.capacity);
            }
        }

        bandwidth_per_attacker
    }
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
