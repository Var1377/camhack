# Client Architecture

## Overview

The client represents a player's laptop in the CamHack game. It is:
- A Raft node (participates in consensus)
- The player's control interface (REST API)
- The final kill target (when capital is captured)
- A **client node** in game state (different from grid nodes)

The client is special: losing your capital triggers a final kill attack on your CLIENT, not on the grid.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                      Client Node                             │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Player Context                           │  │
│  │  - player_id                                          │  │
│  │  - player_name                                        │  │
│  │  - capital_coord                                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Raft Node                                │  │
│  │  - Participates in consensus                          │  │
│  │  - Reads game state                                   │  │
│  │  - Submits player actions as events                  │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              HTTP API (Port 8080)                     │  │
│  │                                                        │  │
│  │  Player Control:                                      │  │
│  │  - POST /join      - Join a game                     │  │
│  │  - GET /my/status  - View my status                  │  │
│  │  - GET /my/nodes   - List my nodes                   │  │
│  │  - POST /my/attack - Command node to attack          │  │
│  │                                                        │  │
│  │  Game Discovery:                                      │  │
│  │  - GET /discover   - Find active games               │  │
│  │  - GET /status     - Connection status               │  │
│  │                                                        │  │
│  │  Real-time Updates:                                   │  │
│  │  - GET /ws         - WebSocket game updates          │  │
│  │                                                        │  │
│  │  Final Kill:                                          │  │
│  │  - GET /finalkill  - Receive attack flood            │  │
│  │                                                        │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Key Differences from Workers

| Aspect | Worker Node | Client Node |
|--------|-------------|-------------|
| Purpose | Grid combat | Player control |
| Attacks | Sends/receives UDP floods | Only receives final kill |
| Game Logic | Leader runs capture detection | Never leader (read-only) |
| Task Type | ECS Fargate | Can run locally |
| Loss Condition | Doesn't lose | Loses when overwhelmed |

## Player Lifecycle

### 1. Start Client

```bash
# Run locally or in ECS
MASTER_URL=http://master-ip:8080 cargo run
```

The client starts in "not joined" state:
- No Raft connection
- No player identity
- Just waiting for `/join` call

### 2. Discover Games

```bash
GET /discover
```

**Response:**
```json
{
  "games": [
    {
      "game_id": "game-001",
      "worker_count": 15
    }
  ]
}
```

Lists all active games from master registry.

### 3. Join Game

```bash
POST /join
{
  "player_name": "Alice",
  "game_id": "game-001"
}
```

**What happens:**
1. Register with master → get random peer
2. Bootstrap/join Raft cluster
3. Find unoccupied hex coordinate
4. Submit `PlayerJoin` event:
   ```rust
   GameEvent::PlayerJoin {
       player_id: generated_id,
       name: "Alice",
       capital_coord: (q, r),
       node_ip: client_ip,
       is_client: true,  // ← KEY: This is a client node
       timestamp: now,
   }
   ```
5. Store player context locally
6. Now connected to game

**Important:** The client sets `is_client: true`, which:
- Stores IP in `game_state.client_ips`
- Marks this node as a "Client" type
- Makes it the target of final kill attacks

### 4. Control Nodes

After joining, the player owns a capital node on the grid.

**View Status:**
```bash
GET /my/status
```
```json
{
  "player_id": 123456,
  "player_name": "Alice",
  "capital_coord": { "q": 0, "r": 0 },
  "alive": true,
  "owned_nodes": 1,
  "is_leader": false
}
```

**List Owned Nodes:**
```bash
GET /my/nodes
```
```json
[
  {
    "coord": { "q": 0, "r": 0 },
    "node_type": "Capital",
    "current_target": null
  }
]
```

**Set Attack Target:**
```bash
POST /my/attack
{
  "target_q": 1,
  "target_r": 0,
  "node_q": 0,  // optional: which node attacks
  "node_r": 0
}
```

This submits a `SetNodeTarget` event through Raft:
```rust
GameEvent::SetNodeTarget {
    node_coord: (0, 0),
    target: Some(Coordinate(1, 0)),
    timestamp: now,
}
```

All workers see this event and start UDP flooding.

### 5. Real-Time Updates

```bash
GET /ws  (upgrade to WebSocket)
```

Streams game state updates:
```json
{
  "log_index": 142,
  "event_count": 142,
  "player_count": 3,
  "node_count": 25,
  "alive_players": 3,
  "latest_event": "NodeCaptured: (2,1) → Player 789"
}
```

Sent every 2 seconds. Used for UI updates.

### 6. Final Kill Attack

When your capital is captured:
1. Your `player.alive` becomes `false`
2. All attacker nodes open WebSocket connections to your client
3. Each connection floods you with 1KB binary messages
4. This happens on the `/finalkill` endpoint

**Endpoint Handler:**
```rust
async fn handle_finalkill_websocket(mut socket: WebSocket) {
    println!("[FinalKill] Attacker connected");
    let mut bytes_received = 0;
    
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Binary(data)) => {
                bytes_received += data.len();
                // Just count, don't respond
            }
            _ => break
        }
    }
    
    println!("Final kill ended: {} bytes", bytes_received);
}
```

The client **passively receives** flood data. It doesn't fight back.

If your client survives 10 seconds → you're still alive (for now).
If your client is overwhelmed → you lose the game.

## Game State

The client reads game state from Raft but rarely writes:

**Read Operations:**
- Get player status
- List owned nodes
- Check alive status
- View grid state

**Write Operations:**
- PlayerJoin (once, on join)
- SetNodeTarget (player commands)

The client **never** runs game logic (no capture detection, no metrics reporting).

## Data Structures

```rust
struct PlayerContext {
    player_id: u64,
    player_name: String,
    capital_coord: NodeCoord,
}

struct ClientState {
    raft_node: Arc<RwLock<Option<Arc<RaftNode>>>>,
    player_context: Arc<RwLock<Option<PlayerContext>>>,
    master_url: Arc<String>,
}
```

## API Reference

### GET /discover

Discover active games from master.

**Response:**
```json
{
  "games": [
    { "game_id": "game-001", "worker_count": 10 }
  ]
}
```

### GET /status

Check if joined to a game.

**Response (not joined):**
```json
{
  "joined": false,
  "message": "Not joined to any game"
}
```

**Response (joined):**
```json
{
  "joined": true,
  "player_id": 123,
  "is_leader": false
}
```

### POST /join

Join a game as a new player.

**Request:**
```json
{
  "player_name": "Alice",
  "game_id": "game-001"
}
```

**Response:**
```json
"Successfully joined game game-001 as Alice"
```

**Errors:**
- `"Already joined to a game"` - Can only join once
- `"Failed to get IP: ..."` - ECS metadata issue
- `"Failed to register with master: ..."` - Master unreachable
- `"Failed to find capital position: ..."` - Grid full (unlikely)

### GET /my/status

Get status of your player.

**Response:**
```json
{
  "player_id": 123456789,
  "player_name": "Alice",
  "capital_coord": { "q": 0, "r": 0 },
  "alive": true,
  "owned_nodes": 5,
  "is_leader": false
}
```

**Errors:**
- `"Not joined to any game. Call POST /join first"` - Must join first

### GET /my/nodes

List all nodes you own.

**Response:**
```json
[
  {
    "coord": { "q": 0, "r": 0 },
    "node_type": "Capital",
    "current_target": "Coordinate(1, 0)"
  },
  {
    "coord": { "q": 1, "r": 1 },
    "node_type": "Regular",
    "current_target": null
  }
]
```

### POST /my/attack

Command a node to attack a target.

**Request:**
```json
{
  "target_q": 1,
  "target_r": 0,
  "node_q": 0,     // optional: defaults to capital
  "node_r": 0
}
```

**Response:**
```json
"Attack target set successfully"
```

**Errors:**
- `"Target must be adjacent to owned node"` - Can only attack neighbors
- `"Node not found"` - Invalid node coordinate
- `"Not joined to any game"` - Must join first

**Validation:**
- Target must be adjacent to a node you own (hexagonal neighbors only)
- Can attack ANY adjacent node:
  - **Neutral nodes** (unoccupied): Spawns new worker via lazy init, requires flooding to capture
  - **Enemy regular nodes**: Standard grid combat
  - **Enemy capitals**: If captured, triggers final kill on their client
- Cannot attack your own nodes

### GET /ws

WebSocket for real-time game updates.

**Upgrade:** Standard WebSocket handshake

**Messages (every 2 seconds):**
```json
{
  "log_index": 250,
  "event_count": 250,
  "player_count": 4,
  "node_count": 30,
  "alive_players": 3,
  "latest_event": "SetNodeTarget: (0,0) → (1,0)"
}
```

### GET /finalkill

WebSocket endpoint for receiving final kill attacks.

**Upgrade:** Standard WebSocket handshake

**Messages:** Binary 1KB flood data

**Behavior:**
- Accept connections from attackers
- Receive flood data passively
- Count bytes for logging
- Connection lasts up to 10 seconds

## Attack Target Specification

When calling `/my/attack`, you can specify:

1. **Coordinate Attack** (normal):
   ```json
   { "target_q": 1, "target_r": 0 }
   ```
   Attacks a specific grid hex. Most common.

2. **Player Attack** (final kill - currently automatic):
   ```json
   { "target_player_id": 456 }
   ```
   Not exposed in client API. Done automatically when capital captured.

## Client Node vs. Grid Node

This is crucial to understand:

**Client Node (this binary):**
- Runs client code
- Player's laptop
- `is_client: true` in PlayerJoin event
- Stored in `game_state.client_ips`
- Target of final kill
- **Loss condition**

**Grid Nodes (workers):**
- Run worker code
- Deployed on ECS
- `is_client: false`
- Stored in `game_state.node_ips`
- Do the actual combat

When you join, you create:
1. A client node (your laptop)
2. A capital node (ECS worker)

The capital is on the grid and fights. Your client just controls it.

## Winning/Losing

**You lose when:**
1. Enemy captures your capital
2. Enemy workers open final kill connections to YOUR CLIENT
3. Your client is overwhelmed for 10 seconds
4. Game marks you as eliminated

**You win when:**
- All other players are eliminated
- You're the last one standing
- Game ends, infrastructure shuts down

## Deployment

### Local Development
```bash
# Set master URL
export MASTER_URL=http://localhost:8080

# Run client
cargo run

# In another terminal, join game
curl -X POST http://localhost:8080/join \
  -H 'Content-Type: application/json' \
  -d '{"player_name":"Alice","game_id":"test-game"}'
```

### ECS Deployment
```bash
# Build & push image
docker build -t client .
docker tag client:latest <account>.dkr.ecr.us-east-1.amazonaws.com/client:latest
docker push <account>.dkr.ecr.us-east-1.amazonaws.com/client:latest

# Register task definition
aws ecs register-task-definition --cli-input-json file://task-definition.json

# Run client
aws ecs run-task \
  --cluster my-cluster \
  --task-definition client \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[subnet-xxx],securityGroups=[sg-xxx],assignPublicIp=ENABLED}" \
  --overrides '{
    "containerOverrides": [{
      "name": "client",
      "environment": [
        {"name":"MASTER_URL","value":"http://MASTER_IP:8080"}
      ]
    }]
  }'
```

## UI Integration

The client is designed for programmatic control. You can build a UI:

### Example: Terminal UI
```bash
# Join game
curl -X POST localhost:8080/join -d '{"player_name":"Alice","game_id":"game1"}'

# Check status
watch -n 1 curl localhost:8080/my/status

# Attack neighbor
curl -X POST localhost:8080/my/attack -d '{"target_q":1,"target_r":0}'
```

### Example: Web UI
```javascript
// Connect to WebSocket
const ws = new WebSocket('ws://CLIENT_IP:8080/ws');
ws.onmessage = (msg) => {
  const state = JSON.parse(msg.data);
  updateUI(state);
};

// Set attack
async function attack(targetQ, targetR) {
  await fetch('/my/attack', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ target_q: targetQ, target_r: targetR })
  });
}
```

### Example: Auto-Player Bot
```python
import requests
import time

client = "http://localhost:8080"

# Join
requests.post(f"{client}/join", json={
    "player_name": "Bot",
    "game_id": "game-001"
})

# Main loop
while True:
    status = requests.get(f"{client}/my/status").json()
    
    if not status["alive"]:
        print("Bot eliminated!")
        break
    
    nodes = requests.get(f"{client}/my/nodes").json()
    
    # Attack random neighbor
    for node in nodes:
        if node["current_target"] is None:
            # Find neighbor and attack
            q, r = node["coord"]["q"], node["coord"]["r"]
            requests.post(f"{client}/my/attack", json={
                "target_q": q + 1,
                "target_r": r,
                "node_q": q,
                "node_r": r
            })
            break
    
    time.sleep(5)
```

## Monitoring

### CloudWatch Logs
If deployed on ECS:
- Log group: `/ecs/client`
- Stream prefix: `client`

### Key Events to Watch

```
# Joining game
=== Joining Game: game-001 ===

# Successfully joined
✓ Successfully joined game: game-001

# Attack sent
[Event] Submitted SetNodeTarget for (0,0) → (1,0)

# Under attack (final kill)
[FinalKill] Attacker connected, receiving flood data...

# Final kill ended
[FinalKill] Disconnected, total received: 1048576 bytes
```

## Troubleshooting

### "Already joined to a game"
- Client only supports one game at a time
- Restart client to join different game

### "Failed to get IP: ..."
- Not running on ECS, or metadata endpoint unreachable
- Set CLIENT_IP env var manually for local testing

### "Target must be adjacent to owned node"
- Can only attack neighbors in hex grid
- Check your nodes with `GET /my/nodes`
- Calculate valid neighbors

### "Not joined to any game"
- Call `POST /join` first
- Check if join succeeded

### "Failed to submit event"
- Not connected to Raft leader
- Raft will retry automatically
- Check network connectivity

### Final kill not working
- Check `/finalkill` endpoint is exposed (port 8080)
- Verify security group allows inbound TCP 8080
- Check CloudWatch logs for connection errors

## Performance

### Resource Usage
- **CPU:** Very low (just API server)
- **Memory:** ~50-100 MB
- **Network:** Only control messages (not flooding)

### Scalability
- One client per player
- Typical game: 2-10 clients
- Each client maintains 1 Raft connection
- Minimal overhead

## Security Notes

- No authentication (demo environment)
- Open HTTP endpoints
- Anyone can join any game
- No rate limiting
- No input validation beyond type checking

For production:
- Add player authentication
- API keys for master
- Rate limit attack commands
- Validate coordinates
- HTTPS/TLS

## Code Structure

```
client/
├── src/
│   └── main.rs          # All client logic (single file)
├── task-definition.json # ECS task definition
└── CLAUDE.md           # This file
```

The client is intentionally simple - a single Rust file with Axum HTTP server.

## Testing

```bash
# Build
cargo build

# Test join flow
cargo run &
CLIENT_PID=$!

sleep 2

curl -X POST http://localhost:8080/join \
  -H 'Content-Type: application/json' \
  -d '{"player_name":"TestPlayer","game_id":"test"}'

curl http://localhost:8080/my/status

kill $CLIENT_PID
```

## Future Enhancements

Potential improvements:
- **Multiple games:** Support switching between games
- **Persistence:** Reconnect to game after restart
- **UI:** Built-in web UI
- **Bots:** AI player mode
- **Spectator mode:** Watch games without joining
- **Replay:** Record and replay games
- **Matchmaking:** Auto-pair players

Currently the client is minimal - just enough to play the game programmatically.
