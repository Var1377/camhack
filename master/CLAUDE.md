# Master Server Architecture

## Overview

The master server is the central orchestrator for CamHack games. It manages:
- ECS task lifecycle (spawning/terminating worker nodes and clients)
- Worker/client registration and peer discovery
- Game cluster tracking
- Infrastructure shutdown on game completion

The master does NOT participate in game state consensus - it's purely an infrastructure manager.

## Key Responsibilities

### 1. Worker Registration & Peer Discovery

When a worker or client starts up, it calls `POST /register`:
```
{
  "worker_id": "worker-abc123",
  "task_arn": "arn:aws:ecs:...",
  "ip": "10.0.1.42",
  "game_id": "game-001"
}
```

The master:
- Stores the worker in the game's cluster registry
- Returns a random peer from the same game for Raft bootstrapping
- If no peers exist, returns `null` (worker becomes cluster founder)

This enables dynamic cluster formation without hardcoded peer lists.

### 2. Task Spawning

**Regular Nodes** (`POST /spawn_workers?count=10&game_id=X`):
- CPU: 256
- Memory: 512 MB
- Task definition: `udp-node`
- Used for grid expansion nodes

**Capital Nodes** (`POST /spawn_workers?count=1&game_id=X&is_capital=true`):
- CPU: 512 (2x)
- Memory: 1024 MB (2x)
- Task definition: `udp-node-capital`
- Used for player capitals (higher capacity)

**Clients** (spawned separately with client task definition):
- Player's laptop representation
- Target of final kill attacks

### 3. Game Cluster Tracking

The master maintains a registry of all active games:
```rust
struct GameCluster {
    game_id: String,
    workers: HashMap<String, WorkerInfo>,
    created_at: SystemTime,
}

struct WorkerInfo {
    worker_id: String,
    task_arn: String,
    ip: String,
    last_heartbeat: SystemTime,
}
```

Each game is isolated - workers from different games never interact.

### 4. Infrastructure Shutdown

When a game ends (one player remaining):
1. Leader worker calls `POST /kill_workers` to terminate all workers
2. Leader calls `POST /kill` to terminate the master itself
3. All ECS tasks are stopped via AWS API
4. Game ends cleanly

## State Management

```rust
struct AppState {
    ecs_client: EcsClient,           // AWS ECS API client
    cluster_name: String,             // ECS cluster name
    task_definition: String,          // Regular node task def
    capital_task_definition: String,  // Capital node task def (2x resources)
    subnet_id: String,                // VPC subnet for tasks
    security_group_id: String,        // Security group for tasks
    games: Arc<RwLock<HashMap<String, GameCluster>>>,
    self_task_arn: Option<String>,   // For self-termination
}
```

## API Endpoints

### GET /games
Lists all active game clusters with worker counts.

### POST /register
Register a worker/client and get a random peer for Raft bootstrapping.

**Request:**
```json
{
  "worker_id": "worker-123",
  "task_arn": "arn:aws:ecs:...",
  "ip": "10.0.1.42",
  "game_id": "game-001"
}
```

**Response:**
```json
{
  "peer_id": "worker-456",
  "peer_ip": "10.0.1.43"
}
```
Or `null` if no peers exist (bootstrap new cluster).

### POST /spawn_workers
Spawn N worker nodes for a game.

**Query params:**
- `count`: Number of workers to spawn (default: 1)
- `game_id`: Game ID (default: "default-game")
- `is_capital`: Use capital task definition (default: false)

**Returns:** Array of spawned task ARNs

### GET /workers
List all workers across all games (debug endpoint).

### POST /kill_workers
Stop all worker tasks in the cluster. Called when game ends.

### POST /kill
Terminate the master task itself. Final cleanup step.

## Network Configuration

The master creates tasks with:
- **Network mode:** `awsvpc` (each task gets its own ENI)
- **Launch type:** Fargate (serverless)
- **Subnet:** Configurable via `SUBNET_ID` env var
- **Security group:** Configurable via `SECURITY_GROUP_ID` env var

Required security group rules:
- TCP 5000 (Raft consensus)
- TCP 8080 (HTTP API & WebSocket)
- UDP 8081 (UDP attack responder)

## Environment Variables

- `CLUSTER_NAME` - ECS cluster name (required)
- `WORKER_TASK_DEFINITION` - Regular node task def name (default: "worker")
- `CAPITAL_TASK_DEFINITION` - Capital node task def name (default: "worker-capital")
- `SUBNET_ID` - VPC subnet ID (required)
- `SECURITY_GROUP_ID` - Security group ID (required)
- `SELF_TASK_ARN` - Master's own task ARN for self-termination (optional)

## Deployment

The master itself runs as an ECS Fargate task:
```bash
# Register task definition
aws ecs register-task-definition --cli-input-json file://master/task-definition.json

# Run master
aws ecs run-task \
  --cluster my-cluster \
  --task-definition master \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[subnet-xxx],securityGroups=[sg-xxx],assignPublicIp=ENABLED}"
```

The master's public IP is used by workers/clients as `MASTER_URL`.

## Scaling

The master is stateless except for in-memory game registries:
- Could be replaced with DynamoDB/Redis for multi-master setups
- Currently single-master architecture
- Handles 100s of workers easily (limited by AWS API rate limits, not master CPU)

## Security Considerations

- No authentication (designed for CTF/demo environments)
- Public endpoints (could add API key auth)
- Direct ECS task control (requires IAM permissions)
- No rate limiting (could add throttling)

For production use, add:
- API authentication
- Rate limiting
- TLS/HTTPS
- VPC-only endpoints
- IAM role separation

## Monitoring

Logs are sent to CloudWatch Logs:
- Log group: `/ecs/master`
- Stream prefix: `master`

Key metrics to monitor:
- Active game count
- Worker count per game
- Registration rate
- Task spawn failures
- AWS API errors

## Failure Modes

**Master crashes:**
- Workers continue running independently
- New workers cannot join (no peer discovery)
- Game can continue if all players already joined

**Worker crashes:**
- Raft handles node failures automatically
- Other workers continue consensus
- Master doesn't track worker health (Raft does)

**Network partition:**
- Raft elects new leader in majority partition
- Minority partition cannot make progress
- Game continues in majority partition

## Code Structure

```
master/
├── src/
│   └── main.rs          # All master logic (single file)
├── task-definition.json # Master's ECS task definition
└── CLAUDE.md           # This file
```

The master is intentionally simple - a single Rust file with Axum HTTP server.
