# Worker Node Architecture

## Overview

Worker nodes are the core of the CamHack game. Each worker is:
- A Raft consensus node (distributed state machine)
- A grid node in the game world
- A UDP flooder/responder
- A game logic processor (when leader)

Workers form a fully decentralized cluster using Raft for consistency, with no single point of failure after bootstrap.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        Worker Node                           │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Raft Module  │  │ Game Logic   │  │ Network Mgr  │      │
│  │              │  │              │  │              │      │
│  │ - Consensus  │  │ - Capture    │  │ - UDP Flood  │      │
│  │ - Log Replic │  │ - Overload   │  │ - ACK Track  │      │
│  │ - Elections  │  │ - Events     │  │ - Metrics    │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                  │                  │               │
│         └──────────────────┴──────────────────┘               │
│                            │                                  │
│                    ┌───────▼───────┐                         │
│                    │  Game State   │                         │
│                    │  (from events)│                         │
│                    └───────────────┘                         │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              HTTP API (Port 8080)                     │  │
│  │  - Event submission                                   │  │
│  │  - Lazy node initialization                          │  │
│  │  - WebSocket flood endpoint (/attack)                │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │           UDP Responder (Port 8081)                   │  │
│  │  - Receives attack packets                            │  │
│  │  - Sends ACKs every 100ms                            │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │           Raft RPC (Port 5000)                        │  │
│  │  - Leader election                                    │  │
│  │  - Log replication                                    │  │
│  │  - Heartbeats                                         │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Core Modules

### 1. Raft Consensus (`raft/`)

Powered by [OpenRaft](https://docs.rs/openraft), provides distributed consensus:

**Key Components:**
- `storage.rs` - In-memory state machine storing game events
- `node_registry.rs` - Dynamic peer discovery
- `api.rs` - Event submission & lazy node initialization

**How it works:**
1. All game events go through Raft (PlayerJoin, SetNodeTarget, NodeCaptured, etc.)
2. Leader accepts event, appends to log, replicates to followers
3. Once majority acknowledges, event is committed
4. All nodes apply committed events to their game state in order
5. Guaranteed consistency: all nodes see same event sequence

**Leader Election:**
- Random timeout (150-300ms)
- Candidate requests votes from peers
- Majority votes needed to become leader
- Only leader processes game logic ticks

**Dynamic Membership:**
- New nodes join via `POST /register` to master
- Get random existing peer, join cluster
- Auto-discover other nodes via Raft membership

### 2. Game Logic (`game/`)

Event-sourced game state derived from Raft log:

**`events.rs`** - Event definitions:
```rust
enum GameEvent {
    PlayerJoin { player_id, capital_coord, is_client, ... },
    SetNodeTarget { node_coord, target, ... },
    NodeCaptured { node_coord, new_owner_id, ... },
    NodeMetricsReport { bandwidth_in, packet_loss, ... },
    NodeInitializationStarted { node_coord, owner_id, ... },
    NodeInitializationComplete { node_coord, node_ip, ... },
}
```

**`state.rs`** - Game state:
```rust
struct GameState {
    players: HashMap<u64, Player>,
    nodes: HashMap<NodeCoord, Node>,
    node_metrics: HashMap<NodeCoord, NodeMetrics>,
    node_ips: HashMap<NodeCoord, String>,      // Grid node IPs
    client_ips: HashMap<u64, String>,          // Client (laptop) IPs
    last_applied_log_index: u64,
    game_over: bool,
}
```

All state is derived by replaying events from Raft log. No external state.

**`logic.rs`** - Game rules (leader-only):
```rust
struct GameLogic {
    config: GameConfig,
    attack_tracker: AttackTracker,
}
```

Every tick (1 second), the leader:
1. Checks all nodes being attacked
2. If packet loss >= 20% for 5+ seconds → NodeCaptured event
3. If capital captured → player loses
4. If only one player alive → game over

**`grid.rs`** - Hexagonal grid:
- Axial coordinate system (q, r)
- Neighbor calculation for adjacency checks
- Attack validation (must own adjacent node)

### 3. Network Manager (`game/network.rs`)

Manages UDP flooding attacks between grid nodes:

**Attack Flow (1-to-1 Grid Combat):**
1. Game state says "Node A attacks Node B"
2. NetworkManager on Node B (defender) auto-discovers its coordinate by matching IP
3. NetworkManager opens a single UDP connection to Node A (the specific attacker)
4. Node A floods Node B with UDP packets
5. This is a 1-to-1 connection (NOT to all nodes owned by A's owner)

**Key Design:**
- Grid attacks are always 1-to-1: single attacker → single defender
- Reverse connection pattern: defender opens UDP to attacker
- Auto-discovery: workers discover their coordinates by matching their IP in game state
- Exception: Final kill is many-to-1 (all attacker nodes → client laptop)

**UDP Flooding:**
- Send 1KB packets as fast as possible (no delay)
- Track sent count in `PacketLossTracker`
- Receive ACKs from target every 100ms
- Calculate real packet loss: `(sent - acked) / sent`

**Metrics Reporting:**
- Every 5 seconds, submit NodeMetricsReport event
- bandwidth_in: bytes/second received
- packet_loss: 0.0-1.0 (ACK-based measurement)
- Used by leader to detect overload & capture

### 4. UDP Module (`game/udp.rs`)

Low-level UDP implementation:

**`udp_responder` (Port 8081):**
```rust
loop {
    recv packet
    track sequence number
    if 100ms elapsed:
        send ACK with highest seq & total count
}
```

**`udp_attacker`:**
```rust
loop {
    send UdpAttackPacket {
        seq: counter,
        timestamp: now,
        payload: [0u8; 1024]
    }
    sent_counter++
}
```

**`ack_receiver`:**
```rust
loop {
    recv ACK
    update acked_counter
}
```

**Packet Loss Calculation:**
```rust
fn calculate_loss(&self) -> f32 {
    let sent = self.sent.load();
    let acked = self.acked.load();
    if sent == 0 { 0.0 } else { (sent - acked) / sent }
}
```

Real-time packet loss based on ACK tracking, not estimated bandwidth.

### 5. Final Kill Manager (`game/finalkill.rs`)

Handles 10-second client kill attacks (WebSocket):

**Trigger:**
When a capital is captured:
1. Victim player loses their capital
2. FinalKillManager activates
3. All attacker nodes open WebSocket to victim's client
4. Flood for exactly 10 seconds
5. If client is overwhelmed → player eliminated

**Implementation:**
```rust
async fn start_final_kill(
    player_id: u64,
    client_ip: String,
    all_attacker_nodes: Vec<NodeCoord>,
) {
    for node in all_attacker_nodes {
        spawn task:
            connect to ws://{client_ip}:8080/finalkill
            send 1KB binary messages in loop
            no delay (true flood)
    }
    
    after 10 seconds:
        stop all connections
}
```

This is separate from UDP grid attacks - only for final kill.

## Main Loop

```rust
loop {
    sleep(1 second)
    
    // Check for capital captures → trigger final kill
    for player in players:
        if !player.alive && capital captured:
            start_final_kill on player's client
    
    // Check for game over
    if game_over && is_leader:
        call master to shutdown all infrastructure
        exit
    
    // Sync UDP attacks with game state
    network_manager.sync_with_game_state(game_state)
    
    // Every 5 seconds: report metrics
    if tick % 5 == 0:
        metrics = network_manager.get_metrics()
        submit NodeMetricsReport events
    
    // Leader only: run game logic
    if is_leader:
        capture_events = game_logic.tick(game_state)
        submit NodeCaptured events
}
```

## Lazy Node Initialization

Nodes are spawned on-demand when players expand:

1. Player calls `POST /my/attack` targeting unowned hex
2. Worker checks if target has an owner
3. If unoccupied, submits `NodeInitializationStarted` event
4. Calls master's `spawn_workers` to create ECS task
5. When new worker starts, submits `NodeInitializationComplete` event
6. Node is now ready for capture/attack

This allows infinite grid expansion without pre-spawning all nodes.

## Node Types

**Regular Node:**
- CPU: 256
- Memory: 512 MB
- Created via lazy initialization
- Standard grid expansion

**Capital Node:**
- CPU: 512 (2x)
- Memory: 1024 MB (2x)
- Player's starting base
- Higher bandwidth capacity (more CPU = more packets/sec)
- When captured, triggers final kill

**Client Node:**
- Player's laptop
- Not a worker (runs client binary)
- Target of final kill attacks
- Game loss condition

## Configuration

### Environment Variables

- `MASTER_URL` - Master server HTTP endpoint
- `WORKER_ID` - Unique worker identifier
- `GAME_ID` - Which game to join
- `RAFT_PORT` - Raft RPC port (default: 5000)
- `GAME_PORT` - HTTP API port (default: 8080)

### Game Config

```rust
struct GameConfig {
    overload_duration_secs: 5,    // Sustained overload needed
    overload_threshold: 0.2,       // 20% packet loss = overloaded
}
```

## API Endpoints

### POST /events
Submit a new game event (goes through Raft):
```json
{
  "event": {
    "SetNodeTarget": {
      "node_coord": { "q": 0, "r": 1 },
      "target": { "Coordinate": { "q": 1, "r": 1 } },
      "timestamp": 1234567890
    }
  }
}
```

### POST /join
Join this node to the game as a player's capital:
```json
{
  "player_name": "Alice",
  "game_id": "game-001"
}
```

### GET /ws
WebSocket endpoint for real-time game state updates.

### GET /attack
WebSocket endpoint for receiving flood data (no longer used - replaced by UDP).

## Network Protocols

### Raft RPC (TCP 5000)
- `AppendEntries` - Log replication
- `RequestVote` - Leader election
- `InstallSnapshot` - Snapshot transfer

### HTTP API (TCP 8080)
- Event submission
- Player join
- Game state queries

### UDP Flooding (UDP 8081)
- Attack packets (1KB payload)
- ACK packets (every 100ms)

### WebSocket Final Kill (TCP 8080)
- Only during final kill phase
- Binary 1KB messages
- Lasts exactly 10 seconds

## Data Flow

### Player Joins
```
Client -> POST /join -> Worker
Worker -> PlayerJoin event -> Raft leader
Leader -> Replicates to followers
All nodes -> Apply event -> Create player & capital node
```

### Player Attacks (Any Adjacent Node)
```
Client -> POST /my/attack -> Worker
Worker -> Validates adjacency (must own adjacent node)
Worker -> SetNodeTarget event -> Raft leader
Leader -> Replicates & commits
All nodes -> Apply event -> Update node.current_target
Target NetworkManager -> Auto-discovers coordinate -> Opens 1-to-1 UDP to attacker
Attacker -> Floods target with UDP packets
Target -> Receives packets -> Reports metrics every 5s
Leader -> Sees packet loss >= 20% for 5s -> NodeCaptured event
```

**Note:** Can attack ANY adjacent node (neutral, regular, or capital). Neutral nodes (owner_id=0) are spawned via lazy initialization and require UDP flooding to capture.

### Capital Captured
```
Leader -> NodeCaptured event (capital) -> All nodes apply
All nodes -> See player.alive = false
All nodes -> Check if final kill needed
One node -> FinalKillManager.start_final_kill()
Attacker nodes -> WebSocket connect to client
Client -> Overwhelmed for 10 seconds -> Player eliminated
```

## Deployment

### Build Docker Image
```bash
cd worker
docker build -t udp-node .
docker tag udp-node:latest <account>.dkr.ecr.us-east-1.amazonaws.com/udp-node:latest
docker push <account>.dkr.ecr.us-east-1.amazonaws.com/udp-node:latest
```

### Register Task Definitions
```bash
# Regular nodes (256/512)
aws ecs register-task-definition --cli-input-json file://task-definition.json

# Capital nodes (512/1024)
aws ecs register-task-definition --cli-input-json file://task-definition-capital.json
```

### Spawn Workers
Via master API:
```bash
# Spawn 10 regular nodes
curl -X POST "http://$MASTER_IP:8080/spawn_workers?count=10&game_id=game-001"

# Spawn 1 capital node
curl -X POST "http://$MASTER_IP:8080/spawn_workers?count=1&game_id=game-001&is_capital=true"
```

## Monitoring

### CloudWatch Logs
- Regular nodes: `/ecs/udp-nodes`
- Capital nodes: `/ecs/udp-nodes-capital`

### Key Metrics
- Raft leader: Look for "Became leader" logs
- Packet loss: NodeMetricsReport events
- Captures: NodeCaptured events
- Final kills: "[FinalKill]" log lines

### Debug Commands
```bash
# Check node status
curl http://$WORKER_IP:8080/status

# View Raft log
# (check logs for applied event count)
```

## Performance Tuning

### UDP Flooding
- No delay between packets (true flood)
- Limited by CPU and network bandwidth
- Capital nodes (2x CPU) = ~2x packets/sec

### Raft Performance
- Heartbeat: 50ms
- Election timeout: 150-300ms
- Max batch size: 1000 entries
- Snapshot after 5000 entries

### Memory Usage
- In-memory Raft log (trimmed after snapshots)
- Game state ~1KB per node
- Should handle 1000s of nodes easily

## Failure Recovery

### Worker Crash
- Raft redistributes load automatically
- New leader elected if crashed node was leader
- Game state reconstructed from log on restart

### Network Partition
- Majority partition continues
- Minority partition cannot commit
- Heals automatically when partition resolves

### Total Cluster Loss
- All game state lost (no persistence)
- Would need snapshot/backup for recovery
- Currently designed for ephemeral games

## Security Notes

- No authentication (demo/CTF environment)
- Open UDP ports (could be DDoS vector)
- Direct memory state (no data validation)
- Raft RPC unencrypted

For production:
- Add TLS for Raft & HTTP
- Authenticate event submissions
- Rate limit UDP/WebSocket
- Validate all events
- Add persistence layer

## Code Structure

```
worker/
├── src/
│   ├── main.rs              # Main loop & initialization
│   ├── game/
│   │   ├── mod.rs           # Game module exports
│   │   ├── events.rs        # Event definitions
│   │   ├── state.rs         # Game state & processing
│   │   ├── logic.rs         # Capture logic (leader-only)
│   │   ├── network.rs       # UDP attack manager
│   │   ├── udp.rs           # UDP flooding implementation
│   │   ├── finalkill.rs     # WebSocket final kill
│   │   └── grid.rs          # Hexagonal grid math
│   ├── raft/
│   │   ├── mod.rs           # Raft exports
│   │   ├── storage.rs       # In-memory state machine
│   │   ├── node_registry.rs # Dynamic peer discovery
│   │   └── api.rs           # Event submission API
│   ├── metadata.rs          # ECS metadata fetching
│   └── registry.rs          # Master registration
├── task-definition.json        # Regular node (256/512)
├── task-definition-capital.json # Capital node (512/1024)
└── CLAUDE.md                   # This file
```

## Testing

```bash
# Build
cargo build --target x86_64-unknown-linux-gnu

# Test
cargo test

# Run locally (needs master)
MASTER_URL=http://localhost:8080 \
WORKER_ID=test-worker \
GAME_ID=test-game \
cargo run
```

## Troubleshooting

**"No peer found":**
- First node in game - becomes cluster founder
- Normal behavior

**"Failed to write event":**
- Not leader - event rejected
- Retry or find leader

**"UDP responder error":**
- Port 8081 already in use
- Check firewall rules

**"Raft election timeout":**
- Network partition
- Check security group allows TCP 5000

**High packet loss but no capture:**
- Check overload duration (needs 5 seconds sustained)
- Check threshold (needs 20%+ loss)
- Verify leader is running game logic
