# Raft Consensus Implementation

## Overview

The worker nodes now implement full Raft consensus using OpenRaft 0.9. This provides:
- **Distributed consensus** across all workers
- **Leader election** for coordination
- **Log replication** ensuring all workers have consistent game state
- **Fault tolerance** - cluster continues operating with majority of nodes alive

## Architecture

### Components

1. **RaftNode** (`src/raft/mod.rs`)
   - Core Raft instance with OpenRaft integration
   - Manages leader election and log replication
   - Bootstrap and join cluster functionality

2. **NodeRegistry** (`src/raft/node_registry.rs`)
   - Thread-safe mapping of NodeId → "IP:PORT"
   - Used for peer discovery and communication

3. **Network Layer** (`src/raft/network.rs`)
   - `GrpcNetworkFactory`: Creates gRPC clients for peer communication
   - `GrpcNetwork`: Handles AppendEntries, Vote, and InstallSnapshot RPCs
   - Client connection pooling for efficiency

4. **gRPC Server** (`src/raft/grpc_server.rs`)
   - Receives Raft RPCs from peers on port 5000
   - Implements RaftService trait for OpenRaft

5. **Storage** (`src/raft/storage.rs`)
   - `MemStorage`: In-memory Raft log and state machine
   - `GameStateMachine`: Stores committed game events
   - OpenRaft 0.9 Adaptor pattern for log/state machine split

6. **HTTP API** (`src/raft/api.rs`)
   - REST endpoints for event submission and queries
   - Runs on port 8080

## HTTP API Endpoints

### POST /events
Submit a game event for consensus.

**Request:**
```json
{
  "event": {
    "Critical": {
      "PlayerJoin": {
        "player_id": 12345,
        "name": "Alice",
        "timestamp": 1234567890
      }
    }
  }
}
```

**Response (Success):**
```json
{
  "success": true,
  "message": "Event committed at log index 42",
  "log_index": 42
}
```

**Response (Not Leader):**
```json
{
  "success": false,
  "message": "Not the leader. Current leader: Some(1)",
  "log_index": null
}
```
Status: 503 SERVICE_UNAVAILABLE

**Note:** Events can only be submitted to the leader. Clients should retry with the leader node.

### GET /events
Retrieve all committed events.

**Response:**
```json
{
  "events": [
    {
      "Critical": {
        "PlayerJoin": {
          "player_id": 12345,
          "name": "Alice",
          "timestamp": 1234567890
        }
      }
    }
  ],
  "count": 1
}
```

### GET /status
Get cluster status for this node.

**Response:**
```json
{
  "node_id": 1234567890,
  "is_leader": true,
  "current_leader": 1234567890,
  "current_term": 5,
  "event_count": 42
}
```

## Event Types

### Critical Events (Require Consensus)
- `PlayerJoin { player_id: u64, name: String, timestamp: u64 }`
- `PlayerLeave { player_id: u64, timestamp: u64 }`
- `ScoreUpdate { player_id: u64, score: i32, timestamp: u64 }`
- `GameStateChange { state: String, timestamp: u64 }`

### Ephemeral Events (Leader-Only, No Consensus)
- `PlayerMove { player_id: u64, x: f32, y: f32, z: f32, timestamp: u64 }`
- `PlayerAction { player_id: u64, action: String, timestamp: u64 }`

## Cluster Formation

### Bootstrapping (First Worker)
```rust
let node = bootstrap_cluster(node_id, my_ip, registry).await?;
```

The first worker:
1. Initializes as single-node cluster
2. Becomes the initial leader
3. Starts gRPC server on port 5000
4. Starts HTTP API on port 8080

### Joining (Subsequent Workers)
```rust
let node = join_cluster(node_id, my_ip, peer_info, registry).await?;
```

Subsequent workers:
1. Register self in node registry
2. Create Raft instance
3. Start gRPC server on port 5000
4. Start HTTP API on port 8080
5. Participate in elections (simplified join flow)

**Note:** Current implementation uses simplified cluster formation. Production deployments should implement proper `add_learner()` → `change_membership()` flow.

## Raft Configuration

Configured in `RaftNode::new()`:
- **Heartbeat interval:** 500ms
- **Election timeout:** 1.5s - 3s (randomized)
- **Snapshot timeout:** 10s
- **Max log entries per batch:** 300
- **Entries kept after snapshot:** 1000

## Deployment

### Environment Variables
- `WORKER_ID`: Unique worker identifier (default: auto-generated)
- `MASTER_URL`: Master server URL for registration

### Ports
- **5000**: Raft gRPC communication (internal)
- **8080**: HTTP API (external)

### ECS Task Definition Updates

The ECS task definition should expose port 8080:
```json
{
  "portMappings": [
    {
      "containerPort": 8080,
      "protocol": "tcp"
    }
  ]
}
```

## Testing

### Unit Tests
```bash
cargo test --target x86_64-unknown-linux-gnu
```

All 17 tests pass:
- Node registry tests (6)
- Network layer tests (4)
- Conversion tests (4)
- gRPC server tests (1)
- API tests (2)

### Integration Testing

1. **Deploy 3 workers** to ECS
2. **Wait for leader election** (check GET /status on each worker)
3. **Submit event to leader:**
   ```bash
   curl -X POST http://<leader-ip>:8080/events \
     -H "Content-Type: application/json" \
     -d '{
       "event": {
         "Critical": {
           "PlayerJoin": {
             "player_id": 1,
             "name": "TestPlayer",
             "timestamp": 1234567890
           }
         }
       }
     }'
   ```
4. **Verify replication** - check GET /events on all workers (should see same event)
5. **Test leader failure** - stop leader task, verify new leader elected
6. **Verify consistency** - all workers should still have the same events

## Storage Model

### In-Memory Storage
- All game events stored in memory
- Survives across leadership changes (replicated)
- Lost on process restart (transient cluster)
- For persistence, implement disk-backed storage

### State Machine
- Ordered list of all committed events
- Applied sequentially from Raft log
- Queryable via GET /events
- Shared across all replicas

## Limitations & Future Improvements

1. **Simplified Join Flow**
   - Current: New nodes participate in elections immediately
   - Recommended: Implement add_learner() → change_membership() flow

2. **In-Memory Storage**
   - Events lost on cluster-wide restart
   - Consider persistent storage backend

3. **No Leader Forwarding**
   - Clients must find leader manually
   - Consider implementing request forwarding

4. **Fixed Cluster Size**
   - Cluster membership is implicit
   - Consider dynamic membership changes

5. **No Authentication**
   - HTTP API is unauthenticated
   - Add auth before production use

## Monitoring

Check cluster health:
```bash
# Get status from all workers
for worker in worker1 worker2 worker3; do
  echo "=== $worker ==="
  curl http://$worker:8080/status
done
```

Expected output shows:
- Same `current_term` across all nodes
- Same `current_leader` across all nodes
- Similar `event_count` across all nodes
- One node with `is_leader: true`

## Architecture Benefits

1. **Consistency**: All workers see the same event sequence
2. **Fault Tolerance**: Cluster survives minority node failures
3. **Leader Election**: Automatic failover on leader failure
4. **Ordered Log**: Events applied in consistent order across replicas
5. **Scalability**: Add more workers without compromising consistency

## Next Steps

1. Deploy to ECS Fargate cluster
2. Test with multiple workers
3. Verify leader election and failover
4. Test event submission and replication
5. Monitor cluster health via /status endpoint
6. Implement proper add_learner flow if needed
7. Consider persistent storage for production
