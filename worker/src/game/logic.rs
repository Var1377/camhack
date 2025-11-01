use super::events::{GameEvent, NodeCoord};
use super::state::GameState;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for game logic
pub struct GameConfig {
    /// How long a node must be overloaded before it's captured (seconds)
    pub overload_duration_secs: u64,
    /// Packet loss threshold to consider a node overloaded (0.0 to 1.0)
    pub overload_threshold: f32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            overload_duration_secs: 10,
            overload_threshold: 0.2, // 20% packet loss
        }
    }
}

/// Track ongoing attacks and their start times
#[derive(Debug, Clone)]
struct AttackTracker {
    /// Map of (target_coord -> (attacker_id, overload_start_time))
    overload_start_times: HashMap<NodeCoord, (u64, u64)>,
}

impl AttackTracker {
    fn new() -> Self {
        Self {
            overload_start_times: HashMap::new(),
        }
    }
}

/// Game logic evaluator - runs on leader only
pub struct GameLogic {
    config: GameConfig,
    attack_tracker: AttackTracker,
}

impl GameLogic {
    pub fn new(config: GameConfig) -> Self {
        Self {
            config,
            attack_tracker: AttackTracker::new(),
        }
    }

    /// Evaluate game state and generate capture events if conditions are met
    /// This should be called periodically by the leader
    pub fn tick(&mut self, game_state: &GameState) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check all nodes that are being attacked
        for node in game_state.nodes.values() {
            let target_coord = node.coord;

            // Find who's attacking this node
            let attackers: Vec<_> = game_state
                .nodes
                .values()
                .filter(|n| n.current_target == Some(target_coord))
                .collect();

            if attackers.is_empty() {
                // No one attacking, clear any tracking
                self.attack_tracker.overload_start_times.remove(&target_coord);
                continue;
            }

            // Multiple attackers: only first attacker succeeds, rest get reflected (per game rules)
            // For now, just take the first attacker
            let attacker = attackers[0];

            // Check if node is overloaded
            if let Some(metrics) = game_state.node_metrics.get(&target_coord) {
                let is_overloaded = metrics.packet_loss >= self.config.overload_threshold;

                if is_overloaded {
                    // Track when overload started
                    let (tracked_attacker, overload_start) = self
                        .attack_tracker
                        .overload_start_times
                        .entry(target_coord)
                        .or_insert((attacker.owner_id, current_time));

                    // Check if sustained long enough
                    let overload_duration = current_time.saturating_sub(*overload_start);
                    if overload_duration >= self.config.overload_duration_secs {
                        // Capture!
                        events.push(GameEvent::NodeCaptured {
                            node_coord: target_coord,
                            new_owner_id: *tracked_attacker,
                            timestamp: current_time,
                        });

                        // Clear tracking after capture
                        self.attack_tracker.overload_start_times.remove(&target_coord);
                    }
                } else {
                    // Not overloaded anymore, reset tracking
                    self.attack_tracker.overload_start_times.remove(&target_coord);
                }
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::events::NodeType;
    use crate::game::state::{Node, NodeMetrics, Player};

    #[test]
    fn test_capture_after_sustained_overload() {
        let mut logic = GameLogic::new(GameConfig {
            overload_duration_secs: 5,
            overload_threshold: 0.5,
        });

        let mut game_state = GameState::new();

        // Add two players
        game_state.players.insert(
            1,
            Player {
                player_id: 1,
                name: "Alice".to_string(),
                capital_coord: NodeCoord::new(0, 0),
                alive: true,
                join_time: 1000,
            },
        );
        game_state.players.insert(
            2,
            Player {
                player_id: 2,
                name: "Bob".to_string(),
                capital_coord: NodeCoord::new(1, 0),
                alive: true,
                join_time: 1001,
            },
        );

        // Add nodes
        game_state.nodes.insert(
            NodeCoord::new(0, 0),
            Node {
                coord: NodeCoord::new(0, 0),
                owner_id: 1,
                node_type: NodeType::Capital,
                capacity: 10_000_000,
                current_target: None,
            },
        );
        game_state.nodes.insert(
            NodeCoord::new(1, 0),
            Node {
                coord: NodeCoord::new(1, 0),
                owner_id: 2,
                node_type: NodeType::Capital,
                capacity: 10_000_000,
                current_target: Some(NodeCoord::new(0, 0)), // Bob attacks Alice
            },
        );

        // Add metrics showing Alice's node is overloaded
        game_state.node_metrics.insert(
            NodeCoord::new(0, 0),
            NodeMetrics {
                bandwidth_in: 20_000_000,
                packet_loss: 0.7, // 70% loss
                timestamp: 2000,
            },
        );

        // First tick - should not capture yet (not sustained)
        let events = logic.tick(&game_state);
        assert_eq!(events.len(), 0);

        // Simulate time passing (advance tracker manually for test)
        if let Some((_, start_time)) = logic.attack_tracker.overload_start_times.get_mut(&NodeCoord::new(0, 0)) {
            *start_time -= 10; // Pretend it started 10 seconds ago
        }

        // Second tick - should capture now (sustained overload)
        let events = logic.tick(&game_state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            GameEvent::NodeCaptured { node_coord, new_owner_id, .. } => {
                assert_eq!(*node_coord, NodeCoord::new(0, 0));
                assert_eq!(*new_owner_id, 2); // Bob captures
            }
            _ => panic!("Expected NodeCaptured event"),
        }
    }
}
