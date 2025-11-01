use crate::raft::conversions::proto::raft_service_server::{RaftService, RaftServiceServer};
use crate::raft::conversions::proto::{
    AppendEntriesRequest as ProtoAppendEntriesRequest,
    AppendEntriesResponse as ProtoAppendEntriesResponse,
    InstallSnapshotRequest as ProtoInstallSnapshotRequest,
    InstallSnapshotResponse as ProtoInstallSnapshotResponse,
    VoteRequest as ProtoVoteRequest, VoteResponse as ProtoVoteResponse,
};
use crate::raft::storage::GameRaftTypeConfig;
use openraft::Raft;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// gRPC service implementation for Raft RPCs
pub struct RaftGrpcService {
    raft: Arc<Raft<GameRaftTypeConfig>>,
}

impl RaftGrpcService {
    /// Create a new gRPC service wrapping a Raft instance
    pub fn new(raft: Arc<Raft<GameRaftTypeConfig>>) -> Self {
        Self { raft }
    }
}

#[tonic::async_trait]
impl RaftService for RaftGrpcService {
    /// Handle AppendEntries RPC - used for log replication and heartbeats
    async fn append_entries(
        &self,
        request: Request<ProtoAppendEntriesRequest>,
    ) -> Result<Response<ProtoAppendEntriesResponse>, Status> {
        let proto_req = request.into_inner();

        // Convert proto request to OpenRaft type
        let raft_req: openraft::raft::AppendEntriesRequest<GameRaftTypeConfig> = proto_req
            .try_into()
            .map_err(|e: anyhow::Error| Status::invalid_argument(e.to_string()))?;

        // Forward to Raft instance
        let raft_resp = self
            .raft
            .append_entries(raft_req)
            .await
            .map_err(|e| Status::internal(format!("Raft append_entries failed: {}", e)))?;

        // Convert OpenRaft response to proto
        let proto_resp: ProtoAppendEntriesResponse = raft_resp.into();

        Ok(Response::new(proto_resp))
    }

    /// Handle RequestVote RPC - used for leader election
    async fn request_vote(
        &self,
        request: Request<ProtoVoteRequest>,
    ) -> Result<Response<ProtoVoteResponse>, Status> {
        let proto_req = request.into_inner();

        // Convert proto request to OpenRaft type
        let raft_req: openraft::raft::VoteRequest<u64> = proto_req.into();

        // Forward to Raft instance
        let raft_resp = self
            .raft
            .vote(raft_req)
            .await
            .map_err(|e| Status::internal(format!("Raft vote failed: {}", e)))?;

        // Convert OpenRaft response to proto
        let proto_resp: ProtoVoteResponse = raft_resp.into();

        Ok(Response::new(proto_resp))
    }

    /// Handle InstallSnapshot RPC - used for transferring snapshots to followers
    async fn install_snapshot(
        &self,
        request: Request<ProtoInstallSnapshotRequest>,
    ) -> Result<Response<ProtoInstallSnapshotResponse>, Status> {
        let proto_req = request.into_inner();

        // Convert proto request to OpenRaft type
        let raft_req: openraft::raft::InstallSnapshotRequest<GameRaftTypeConfig> = proto_req
            .try_into()
            .map_err(|e: anyhow::Error| Status::invalid_argument(e.to_string()))?;

        // Forward to Raft instance
        let raft_resp = self
            .raft
            .install_snapshot(raft_req)
            .await
            .map_err(|e| Status::internal(format!("Raft install_snapshot failed: {}", e)))?;

        // Convert OpenRaft response to proto
        let proto_resp: ProtoInstallSnapshotResponse = raft_resp.into();

        Ok(Response::new(proto_resp))
    }
}

/// Start the gRPC server for Raft communication
/// Returns a JoinHandle that can be awaited or aborted
pub async fn start_grpc_server(
    raft: Arc<Raft<GameRaftTypeConfig>>,
    addr: String,
) -> Result<tokio::task::JoinHandle<Result<(), tonic::transport::Error>>, Box<dyn std::error::Error>> {
    let service = RaftGrpcService::new(raft);
    let server = RaftServiceServer::new(service);

    let socket_addr = addr
        .parse()
        .map_err(|e| format!("Invalid address {}: {}", addr, e))?;

    println!("Starting Raft gRPC server on {}", socket_addr);

    let handle = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(server)
            .serve(socket_addr)
            .await
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_server_address_parsing() {
        // Test that valid addresses parse correctly
        let valid_addr = "127.0.0.1:5000";
        let result: Result<std::net::SocketAddr, _> = valid_addr.parse();
        assert!(result.is_ok());

        // Test that invalid addresses fail
        let invalid_addr = "not-an-address";
        let result: Result<std::net::SocketAddr, _> = invalid_addr.parse();
        assert!(result.is_err());
    }

    // Note: Full integration test for RaftGrpcService requires a properly
    // initialized Raft instance, which will be tested in Phase 4 when we
    // wire up the complete Raft node implementation
}
