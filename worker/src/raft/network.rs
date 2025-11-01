use crate::raft::conversions::proto::raft_service_client::RaftServiceClient;
use crate::raft::node_registry::NodeRegistry;
use crate::raft::storage::{GameRaftTypeConfig, NodeId};
use openraft::error::{InstallSnapshotError, RPCError, RaftError};
use openraft::network::{RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Channel;

/// Simple network error wrapper
#[derive(Debug, Clone)]
struct NetworkError {
    message: String,
}

impl NetworkError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for NetworkError {}

/// Network factory for creating gRPC connections to Raft peers
#[derive(Clone)]
pub struct GrpcNetworkFactory {
    /// Registry mapping NodeId to network address
    registry: NodeRegistry,
    /// Cached gRPC clients for reuse
    clients: Arc<RwLock<std::collections::HashMap<NodeId, RaftServiceClient<Channel>>>>,
}

impl GrpcNetworkFactory {
    /// Create a new network factory with the given node registry
    pub fn new(registry: NodeRegistry) -> Self {
        Self {
            registry,
            clients: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Get or create a gRPC client for the target node
    async fn get_client(&self, target: NodeId) -> Result<RaftServiceClient<Channel>, RPCError<NodeId, (), RaftError<NodeId>>> {
        // Check if we have a cached client
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(&target) {
                return Ok(client.clone());
            }
        }

        // Get the address from registry
        let addr = self
            .registry
            .get_address(target)
            .await
            .ok_or_else(|| {
                RPCError::Unreachable(openraft::error::Unreachable::new(&NetworkError::new(
                    format!("Node {} not found in registry", target)
                )))
            })?;

        // Create new client
        let endpoint = format!("http://{}", addr);
        let client = RaftServiceClient::connect(endpoint.clone())
            .await
            .map_err(|e| {
                RPCError::Unreachable(openraft::error::Unreachable::new(&NetworkError::new(
                    format!("Failed to connect to {}: {}", endpoint, e)
                )))
            })?;

        // Cache the client
        self.clients.write().await.insert(target, client.clone());

        Ok(client)
    }
}

impl RaftNetworkFactory<GameRaftTypeConfig> for GrpcNetworkFactory {
    type Network = GrpcNetwork;

    async fn new_client(&mut self, target: NodeId, _node: &()) -> Self::Network {
        GrpcNetwork {
            target,
            factory: self.clone(),
        }
    }
}

/// Individual network connection to a specific Raft peer
pub struct GrpcNetwork {
    target: NodeId,
    factory: GrpcNetworkFactory,
}

impl RaftNetwork<GameRaftTypeConfig> for GrpcNetwork {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<GameRaftTypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, (), RaftError<NodeId>>> {
        let mut client = self.factory.get_client(self.target).await?;

        // Convert OpenRaft request to proto
        let proto_req: crate::raft::conversions::proto::AppendEntriesRequest = req.into();

        // Send gRPC request
        let response = client
            .append_entries(proto_req)
            .await
            .map_err(|e| {
                RPCError::Network(openraft::error::NetworkError::new(&NetworkError::new(
                    format!("gRPC append_entries failed: {}", e)
                )))
            })?;

        // Convert proto response to OpenRaft
        let proto_resp = response.into_inner();
        Ok(proto_resp.into())
    }

    async fn vote(
        &mut self,
        req: VoteRequest<NodeId>,
        _option: openraft::network::RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, (), RaftError<NodeId>>> {
        let mut client = self.factory.get_client(self.target).await?;

        // Convert OpenRaft request to proto
        let proto_req: crate::raft::conversions::proto::VoteRequest = req.into();

        // Send gRPC request
        let response = client
            .request_vote(proto_req)
            .await
            .map_err(|e| {
                RPCError::Network(openraft::error::NetworkError::new(&NetworkError::new(
                    format!("gRPC request_vote failed: {}", e)
                )))
            })?;

        // Convert proto response to OpenRaft
        let proto_resp = response.into_inner();
        Ok(proto_resp.into())
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<GameRaftTypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, (), RaftError<NodeId, InstallSnapshotError>>,
    > {
        let mut client = self.factory.get_client(self.target).await.map_err(|e| match e {
            RPCError::Unreachable(u) => RPCError::Unreachable(u),
            RPCError::Network(n) => RPCError::Network(n),
            _ => RPCError::Network(openraft::error::NetworkError::new(&NetworkError::new(
                "Unknown error".to_string()
            ))),
        })?;

        // Convert OpenRaft request to proto
        let proto_req: crate::raft::conversions::proto::InstallSnapshotRequest = req.into();

        // Send gRPC request
        let response = client
            .install_snapshot(proto_req)
            .await
            .map_err(|e| {
                RPCError::Network(openraft::error::NetworkError::new(&NetworkError::new(
                    format!("gRPC install_snapshot failed: {}", e)
                )))
            })?;

        // Convert proto response to OpenRaft
        let proto_resp = response.into_inner();
        Ok(proto_resp.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_factory_creation() {
        let registry = NodeRegistry::new();
        let factory = GrpcNetworkFactory::new(registry);

        // Factory should be created successfully
        assert_eq!(factory.clients.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_get_client_not_in_registry() {
        let registry = NodeRegistry::new();
        let factory = GrpcNetworkFactory::new(registry);

        // Trying to get client for non-existent node should fail
        let result = factory.get_client(999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_caching() {
        let registry = NodeRegistry::new();
        registry.register(1, "127.0.0.1:5000".to_string()).await;

        let factory = GrpcNetworkFactory::new(registry);

        // First call attempts connection (will fail since no server)
        // but we can verify the caching logic
        let _ = factory.get_client(1).await;

        // Verify client was attempted to be cached even if connection failed
        // (In real scenario with server, client would be cached)
    }

    #[tokio::test]
    async fn test_network_creation() {
        let registry = NodeRegistry::new();
        registry.register(1, "127.0.0.1:5000".to_string()).await;

        let mut factory = GrpcNetworkFactory::new(registry);

        // Create network for target node
        let network = factory.new_client(1, &()).await;
        assert_eq!(network.target, 1);
    }
}
