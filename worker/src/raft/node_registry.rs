use crate::raft::storage::NodeId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Registry mapping NodeId to network address (IP:PORT)
/// Thread-safe for concurrent access from multiple Raft network connections
#[derive(Clone)]
pub struct NodeRegistry {
    nodes: Arc<RwLock<HashMap<NodeId, String>>>,
}

impl NodeRegistry {
    /// Create a new empty node registry
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a node with its network address
    /// Format: "IP:PORT" (e.g., "10.0.1.5:5000")
    pub async fn register(&self, node_id: NodeId, addr: String) {
        self.nodes.write().await.insert(node_id, addr);
    }

    /// Get the network address for a given node ID
    /// Returns None if node is not registered
    pub async fn get_address(&self, node_id: NodeId) -> Option<String> {
        self.nodes.read().await.get(&node_id).cloned()
    }

    /// Remove a node from the registry
    /// Currently unused but planned for graceful shutdown implementation
    #[allow(dead_code)]
    pub async fn unregister(&self, node_id: NodeId) -> Option<String> {
        self.nodes.write().await.remove(&node_id)
    }

    /// Get all registered nodes
    /// Currently unused but planned for cluster visibility endpoints
    #[allow(dead_code)]
    pub async fn get_all_nodes(&self) -> Vec<(NodeId, String)> {
        self.nodes
            .read()
            .await
            .iter()
            .map(|(id, addr)| (*id, addr.clone()))
            .collect()
    }

    /// Get the number of registered nodes
    /// Currently unused but planned for monitoring/status endpoints
    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.nodes.read().await.len()
    }
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = NodeRegistry::new();

        registry.register(1, "10.0.1.5:5000".to_string()).await;
        registry.register(2, "10.0.1.6:5000".to_string()).await;

        assert_eq!(registry.get_address(1).await, Some("10.0.1.5:5000".to_string()));
        assert_eq!(registry.get_address(2).await, Some("10.0.1.6:5000".to_string()));
        assert_eq!(registry.get_address(99).await, None);
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = NodeRegistry::new();

        registry.register(1, "10.0.1.5:5000".to_string()).await;
        assert!(registry.get_address(1).await.is_some());

        let removed = registry.unregister(1).await;
        assert_eq!(removed, Some("10.0.1.5:5000".to_string()));
        assert!(registry.get_address(1).await.is_none());
    }

    #[tokio::test]
    async fn test_get_all_nodes() {
        let registry = NodeRegistry::new();

        registry.register(1, "10.0.1.5:5000".to_string()).await;
        registry.register(2, "10.0.1.6:5000".to_string()).await;
        registry.register(3, "10.0.1.7:5000".to_string()).await;

        let all_nodes = registry.get_all_nodes().await;
        assert_eq!(all_nodes.len(), 3);
        assert!(all_nodes.contains(&(1, "10.0.1.5:5000".to_string())));
        assert!(all_nodes.contains(&(2, "10.0.1.6:5000".to_string())));
        assert!(all_nodes.contains(&(3, "10.0.1.7:5000".to_string())));
    }

    #[tokio::test]
    async fn test_len() {
        let registry = NodeRegistry::new();

        assert_eq!(registry.len().await, 0);

        registry.register(1, "10.0.1.5:5000".to_string()).await;
        assert_eq!(registry.len().await, 1);

        registry.register(2, "10.0.1.6:5000".to_string()).await;
        assert_eq!(registry.len().await, 2);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let registry = NodeRegistry::new();
        let registry_clone = registry.clone();

        // Spawn multiple tasks that access the registry concurrently
        let handle1 = tokio::spawn(async move {
            for i in 0..10 {
                registry_clone.register(i, format!("10.0.1.{}:5000", i)).await;
            }
        });

        let registry_clone2 = registry.clone();
        let handle2 = tokio::spawn(async move {
            for i in 10..20 {
                registry_clone2.register(i, format!("10.0.1.{}:5000", i)).await;
            }
        });

        handle1.await.unwrap();
        handle2.await.unwrap();

        assert_eq!(registry.len().await, 20);
    }

    #[tokio::test]
    async fn test_overwrite_registration() {
        let registry = NodeRegistry::new();

        registry.register(1, "10.0.1.5:5000".to_string()).await;
        registry.register(1, "10.0.1.99:5000".to_string()).await; // Overwrite

        assert_eq!(registry.get_address(1).await, Some("10.0.1.99:5000".to_string()));
        assert_eq!(registry.len().await, 1); // Still only one node
    }
}
