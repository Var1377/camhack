# CamHack - Distributed Network Flooding Game

## What Is This?

CamHack is a competitive multiplayer game where players capture territory on a hexagonal grid by **literally flooding each other's servers with UDP packets**. The game mechanics are on real network infrastructure:

- **Grid nodes** are actual ECS Fargate tasks running on AWS
- **Attacks** are real UDP floods measuring actual packet loss via ACKs
- **Capacity** is determined by CPU/memory allocation (not hardcoded)
- **Winning** requires overwhelming your opponent's client with network traffic

This is a hacking simulation where the game state IS the infrastructure state.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                          Master Server                              │
│  - Spawns/kills ECS tasks                                           │
│  - Registers workers for peer discovery                             │
│  - Tracks active games                                              │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    │               │               │
        ┌───────────▼──────┐  ┌────▼─────┐  ┌─────▼────────┐
        │  Client (Alice)  │  │  Client  │  │ Client (Bob) │
        │  - Controls      │  │  (Carol) │  │ - Controls   │
        │  - Receives      │  │          │  │ - Receives   │
        │    final kill    │  │          │  │   final kill │
        └────────┬─────────┘  └────┬─────┘  └──────┬───────┘
                 │                 │                │
                 │   Issues commands (REST API)     │
                 │                 │                │
        ┌────────▼─────────────────▼────────────────▼───────┐
        │                                                     │
        │           Raft Consensus Cluster                   │
        │    (All workers + all clients participate)         │
        │                                                     │
        │  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
        │  │ Worker 1 │  │ Worker 2 │  │ Worker 3 │  ...   │
        │  │ (Capital)│  │ (Regular)│  │ (Regular)│        │
        │  │ 512 CPU  │  │ 256 CPU  │  │ 256 CPU  │        │
        │  └────┬─────┘  └────┬─────┘  └────┬─────┘        │
        │       │             │             │               │
        │       └─────────────┼─────────────┘               │
        │                     │                             │
        │         UDP Flooding (Port 8081)                  │
        │      ← 1KB packets, ACK tracking →                │
        │                                                     │
        └─────────────────────────────────────────────────────┘
```

## Core Components

### 1. Master (Infrastructure Manager)
- **Purpose:** Spawn and kill ECS tasks, coordinate worker discovery
- **Port:** 8080 (HTTP)
- **Language:** Rust (Axum)
- **Persistence:** None (in-memory game registry)
- **Failure Impact:** Workers can continue, but new players can't join

### 2. Worker (Grid Node)
- **Purpose:** Grid combat, consensus, game logic
- **Ports:** 5000 (Raft), 8080 (HTTP), 8081 (UDP)
- **Language:** Rust (OpenRaft, Tokio)
- **Types:** Regular (256 CPU, 512 MB) or Capital (512 CPU, 1024 MB)
- **State:** Replicated via Raft (eventually consistent)

### 3. Client (Player's Laptop)
- **Purpose:** Player control, final kill target
- **Port:** 8080 (HTTP + WebSocket)
- **Language:** Rust (Axum)
- **Special:** Participates in Raft but doesn't run game logic
- **Loss Condition:** When overwhelmed by final kill attack

## Game Lifecycle

### Phase 1: Infrastructure Setup

```bash
# 1. Start master
aws ecs run-task --task-definition master --cluster camhack
# Note master's public IP

# 2. Master is now listening on port 8080
# Workers and clients will register with it
```

### Phase 2: Players Join

**Player Alice joins:**
```bash
# 1. Start client (locally or on ECS)
MASTER_URL=http://MASTER_IP:8080 ./client

# 2. Join game via API
curl -X POST http://localhost:8080/join -d '{
  "player_name": "Alice",
  "game_id": "game-001"
}'
```

**What happens internally:**

```
Client → POST /register to master
Master → Returns random peer (or null if first)
Client → Bootstraps/joins Raft cluster
Client → Finds unoccupied hex coordinate
Client → Submits PlayerJoin event {
    player_id: 123,
    name: "Alice",
    capital_coord: (0, 0),
    node_ip: "10.0.1.42",
    is_client: true,  ← Client flag
    timestamp: now
}
Raft Leader → Replicates event to all nodes
All Nodes → Apply event → Create player & capital
Master ← (called by client) Spawn capital worker with is_capital=true
ECS → Launches 512 CPU / 1024 MB task
New Worker → Registers with master
New Worker → Joins Raft cluster
New Worker → Sees PlayerJoin event → Becomes Alice's capital
```

**Player Bob joins:**
- Same process
- Gets different capital coordinate (e.g., (5, 0))
- Now 2 players in the game

### Phase 3: Grid Combat

**Alice attacks Bob's capital:**

```bash
curl -X POST http://ALICE_CLIENT:8080/my/attack -d '{
  "target_q": 5,
  "target_r": 0
}'
```
=
**Attack flow:**

```
1. Alice's client submits SetNodeTarget event:
   GameEvent::SetNodeTarget {
       node_coord: (0, 0),           ← Alice's capital
       target: Coordinate(5, 0),      ← Bob's capital
       timestamp: now
   }

2. Raft leader replicates to all nodes

3. All nodes apply event:
   - Alice's capital node: current_target = Some(Coordinate(5, 0))

4. NetworkManager on Bob's capital sees the attack:
   - Auto-discovers its coordinate by matching IP in game state
   - Opens single UDP connection to Alice's capital (the specific attacker)
   - This is 1-to-1: only to the attacking node, NOT all Alice's nodes

5. Alice's capital floods Bob's capital:
   - Sends 1KB UDP packets to Bob's capital (port 8081)
   - No delay between packets (true flood)
   - Tracks sent count

6. Bob's capital receives packets:
   - UDP responder on port 8081 receives
   - Tracks sequence numbers
   - Sends ACK every 100ms with (highest_seq, total_received)

7. Alice's capital receives ACKs:
   - Updates acked counter
   - Calculates packet loss: (sent - acked) / sent

8. Bob's capital reports metrics (every 5 seconds):
   GameEvent::NodeMetricsReport {
       node_coord: (5, 0),
       bandwidth_in: 2_500_000,      ← bytes/sec
       packet_loss: 0.35,             ← 35% loss!
       timestamp: now
   }

9. Raft leader runs game logic (every 1 second):
   - Sees Bob's capital has 35% packet loss
   - Starts overload timer
   - After 5 seconds of sustained 20%+ loss:
   
   GameEvent::NodeCaptured {
       node_coord: (5, 0),
       new_owner_id: Alice's ID,
       timestamp: now
   }

10. All nodes apply NodeCaptured:
    - Bob's capital: owner_id = Alice
    - Bob's capital: node_type = Regular (was Capital)
    - Bob's player: alive = false
```

**Key mechanics:**
- **Adjacency:** Can only attack neighbors in hex grid
- **Overload:** 20% packet loss for 5 seconds = capture
- **Capacity:** Higher CPU = can send more packets/sec = higher capacity
- **Real measurement:** Packet loss measured via ACKs, not estimated

### Phase 4: Final Kill

When Bob's capital is captured, the **final kill** phase triggers:

```
1. All workers see NodeCaptured event for capital:
   - Bob's player.alive = false
   - Capital was captured by Alice

2. Workers detect final kill condition:
   - Player lost capital
   - Has client IP in game_state.client_ips
   - Trigger FinalKillManager

3. FinalKillManager activates:
   - Get all nodes owned by Alice
   - For each Alice node:
     - Open WebSocket to ws://BOB_CLIENT_IP:8080/finalkill
     - Send 1KB binary messages in loop
     - No delay (true flood)

4. Bob's client receives flood:
   - /finalkill endpoint accepts WebSocket connections
   - Receives flood data passively
   - Just counts bytes (no fighting back)

5. After 10 seconds:
   - All WebSocket connections close
   - If Bob's client survived → still in game (temporary reprieve)
   - If Bob's client was overwhelmed → Bob loses

6. Game checks win condition:
   - If only Alice alive → game over
   - Alice wins!
```

**Final kill details:**
- **Duration:** Exactly 10 seconds
- **Protocol:** WebSocket (not UDP) - reverse connection
- **Target:** Bob's CLIENT (his laptop), not the grid
- **Intensity:** All of Alice's nodes attack simultaneously
- **Purpose:** Finish off the player after capital falls

### Phase 5: Game End

```
1. Leader detects game_over:
   - Only one player.alive = true
   - Sets game_state.game_over = true

2. Leader calls master:
   POST http://MASTER_IP:8080/kill_workers
   → Master stops all worker tasks

3. Leader calls master:
   POST http://MASTER_IP:8080/kill
   → Master stops itself

4. All infrastructure terminates:
   - Workers exit
   - Master exits
   - Clients can exit
   - AWS resources cleaned up
```

## Client Lifecycle (Player's Perspective)

### 1. Start Client
```bash
# Run locally
MASTER_URL=http://MASTER_IP:8080 cargo run

# Or deploy on ECS
aws ecs run-task --task-definition client ...
```

**State:** Not connected to any game

### 2. Discover Games
```bash
curl http://localhost:8080/discover
```

**Response:**
```json
{
  "games": [
    {"game_id": "game-001", "worker_count": 10},
    {"game_id": "game-002", "worker_count": 5}
  ]
}
```

### 3. Join Game
```bash
curl -X POST http://localhost:8080/join -d '{
  "player_name": "Alice",
  "game_id": "game-001"
}'
```

**What you get:**
- Player ID (unique identifier)
- Capital node on grid (spawned worker)
- Raft membership (part of consensus)
- Client IP registered (for final kill)

**State:** Connected to game, own 1 capital node

### 4. Check Status
```bash
curl http://localhost:8080/my/status
```

**Response:**
```json
{
  "player_id": 123456,
  "player_name": "Alice",
  "capital_coord": {"q": 0, "r": 0},
  "alive": true,
  "owned_nodes": 1,
  "is_leader": false
}
```

### 5. List Your Nodes
```bash
curl http://localhost:8080/my/nodes
```

**Response:**
```json
[
  {
    "coord": {"q": 0, "r": 0},
    "node_type": "Capital",
    "current_target": null
  }
]
```

### 6. Attack Enemy
```bash
# Attack the hex to your right
curl -X POST http://localhost:8080/my/attack -d '{
  "target_q": 1,
  "target_r": 0
}'
```

**Validation:**
- Target must be adjacent to your node
- Will start UDP flood immediately
- Can attack unoccupied hex (lazy init) or enemy node

### 7. Expand Territory

**Attack unoccupied hex:**
```bash
curl -X POST http://localhost:8080/my/attack -d '{
  "target_q": 1,
  "target_r": 0
}'
```

**What happens:**
1. SetNodeTarget event submitted
2. Worker detects target is unoccupied
3. Submits NodeInitializationStarted event
4. Calls master to spawn new worker
5. New worker starts, joins Raft
6. Submits NodeInitializationComplete event
7. Node is now attackable
8. After overload duration → captured by you
9. You now own 2 nodes

### 8. Real-Time Updates

**Connect WebSocket:**
```javascript
const ws = new WebSocket('ws://localhost:8080/ws');
ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  console.log(update);
  // {
  //   "log_index": 142,
  //   "player_count": 3,
  //   "alive_players": 2,
  //   "latest_event": "NodeCaptured: (1,0) → Player 123"
  // }
};
```

**Updates every 2 seconds**

### 9. Under Attack (Final Kill)

**When your capital is captured:**

```
[FinalKill] Attacker connected, receiving flood data...
[FinalKill] Attacker connected, receiving flood data...
[FinalKill] Attacker connected, receiving flood data...
(Multiple connections from all enemy nodes)

... 10 seconds of flooding ...

[FinalKill] Disconnected, total received: 10485760 bytes
[FinalKill] Disconnected, total received: 10485760 bytes
[FinalKill] Disconnected, total received: 10485760 bytes
```

**Outcome:**
- If your client stays responsive → you survive (for now)
- If your client crashes or hangs → you lose
- No way to fight back, just endure

### 10. Win or Lose

**You win:**
```json
{
  "alive": true,
  "owned_nodes": 25,
  "alive_players": 1  ← You're the last one
}
```
Game ends, infrastructure shuts down.

**You lose:**
```json
{
  "alive": false,
  "owned_nodes": 0
}
```
You're eliminated, can watch others fight.

## Key Mechanics Explained

### Hexagonal Grid

```
    (0,1) -- (1,1)
   /    \  /    \
(0,0) -- (1,0) -- (2,0)
   \    /  \    /
   (0,-1) - (1,-1)
```

- **Coordinates:** Axial system (q, r)
- **Neighbors:** 6 adjacent hexes
- **Adjacency rule:** Can only attack neighbors (must own adjacent node)
- **Attack targets:** Can attack ANY adjacent node:
  - **Neutral nodes** (unoccupied): Triggers lazy initialization, spawns worker, requires UDP flooding to capture
  - **Enemy regular nodes**: Standard 1-to-1 UDP combat
  - **Enemy capitals**: If captured, triggers final kill on their client
- **Grid expansion strategy:** Grab neutral territory first to build your army, then attack enemies

### UDP Flooding & ACK Tracking

**Traditional approach (not used):**
- Estimate capacity (e.g., 10 Mbps)
- Count bandwidth sent
- If sent > capacity → overloaded
- **Problem:** Fake, not real measurement

**Our approach:**
```
Attacker:
  loop:
    send packet with seq=N
    sent_counter++

Target:
  loop:
    receive packet with seq=N
    store seq
    
    every 100ms:
      send ACK(highest_seq, total_received)

Attacker:
  on ACK received:
    acked_counter = total_received

Packet Loss = (sent - acked) / sent
```

- **Real measurement:** Actual packet loss on the network
- **Dynamic capacity:** Depends on CPU/network, not hardcoded
- **Fair:** Both players limited by same infrastructure

### Attack Connection Patterns

**1-to-1 Grid Combat (Standard Attacks):**
```
Alice's Node (0,0) attacks Bob's Node (1,0):
  - Reverse connection: Bob's node opens UDP to Alice's specific attacking node
  - NOT to all of Alice's nodes - only the one attacking
  - 1-to-1: single attacker floods single defender
  - Protocol: UDP with ACK tracking
```

**Many-to-1 Final Kill (Capital Lost):**
```
Bob loses capital → Final kill triggers:
  - ALL of Alice's nodes connect to Bob's client
  - Protocol: WebSocket (not UDP)
  - Target: Bob's laptop (client), not grid nodes
  - Duration: Exactly 10 seconds
  - This is the ONLY time many nodes attack one target
```

**Key Design:**
- Grid combat is always 1-to-1
- Final kill is many-to-1 (coordinated overwhelming attack)
- Defender initiates the connection (reverse pattern)

### Capital vs Regular Nodes

| Attribute | Regular | Capital |
|-----------|---------|---------|
| CPU | 256 | 512 (2x) |
| Memory | 512 MB | 1024 MB (2x) |
| Task Definition | `udp-node` | `udp-node-capital` |
| Spawned When | Lazy init (expansion) | Player join |
| Capacity | Standard | Higher (~2x packets/sec) |
| Loss Impact | Just territory | Triggers final kill |

### Client vs Grid Node

**Client Node (your laptop):**
- Runs client binary
- Port 8080 (HTTP + WS)
- Controls grid nodes
- Receives final kill
- **Loss condition**

**Grid Node (ECS worker):**
- Runs worker binary
- Ports 5000, 8080, 8081
- Fights on the grid
- Sends/receives UDP
- **Territory**

Critical: You lose when your CLIENT is killed, not when you lose territory.

### Overload & Capture

```
Time: 0s  → Attack starts
Time: 1s  → packet_loss = 0.15 (15%)
Time: 2s  → packet_loss = 0.22 (22%) ← Overload starts
Time: 3s  → packet_loss = 0.25 (25%)
Time: 4s  → packet_loss = 0.23 (23%)
Time: 5s  → packet_loss = 0.21 (21%)
Time: 6s  → packet_loss = 0.24 (24%)
Time: 7s  → packet_loss = 0.26 (26%) ← 5 seconds sustained
         → NodeCaptured event!
```

**Rules:**
- Threshold: 20% packet loss
- Duration: 5 seconds sustained
- Measurement: Real ACK-based tracking
- Action: Node captured, ownership changes

### Raft Consensus

**Why Raft?**
- Ensures all players see the same game state
- No central server (fully distributed)
- Survives node failures
- Leader election automatic

**How it works:**
```
All events → Raft leader
Leader → Appends to log
Leader → Replicates to followers
Majority ACK → Event committed
All nodes → Apply committed events in order
```

**Result:** Guaranteed consistency

## Quick Start

### Minimal Setup (3 Players)

```bash
# 1. Start master
aws ecs run-task --task-definition master --cluster camhack
export MASTER_IP=<public IP>

# 2. Alice joins
MASTER_URL=http://$MASTER_IP:8080 ./client &
sleep 2
curl -X POST http://localhost:8080/join \
  -d '{"player_name":"Alice","game_id":"test"}'

# 3. Bob joins (different machine/container)
MASTER_URL=http://$MASTER_IP:8080 ./client &
sleep 2
curl -X POST http://localhost:8081/join \
  -d '{"player_name":"Bob","game_id":"test"}'

# 4. Carol joins
MASTER_URL=http://$MASTER_IP:8080 ./client &
sleep 2
curl -X POST http://localhost:8082/join \
  -d '{"player_name":"Carol","game_id":"test"}'

# 5. Alice attacks Bob's capital
curl http://localhost:8080/my/status  # Get Bob's coord
curl -X POST http://localhost:8080/my/attack \
  -d '{"target_q":BOB_Q,"target_r":BOB_R}'

# 6. Watch the flood
# Check CloudWatch logs for UDP flooding
# Wait for capture (5 seconds of overload)
# Bob's capital falls → final kill triggers
# Bob's client gets flooded for 10 seconds
# If Bob's client survives, he's still in (temporarily)

# 7. Carol attacks Alice...
# Last one standing wins!
```

### Local Development

```bash
# Terminal 1: Mock master (or run real one)
cd master && cargo run

# Terminal 2: Alice's client
cd client && MASTER_URL=http://localhost:8080 cargo run
# Then: curl -X POST localhost:8080/join -d '{"player_name":"Alice","game_id":"local"}'

# Terminal 3: Bob's client  
cd client && MASTER_URL=http://localhost:8080 cargo run
# Run on different port or different machine

# Watch logs for game events
```

## Architecture Patterns

### Event Sourcing
All game state derived from events:
```
PlayerJoin → Player created, capital spawned
SetNodeTarget → Attack starts
NodeMetricsReport → Leader tracks overload
NodeCaptured → Ownership changes
```

**Benefits:**
- Complete game replay
- Audit trail
- Time travel debugging
- No state synchronization bugs

### Distributed Consensus
Raft ensures all nodes agree:
```
Client → Event → Leader → Log → Followers → Commit → Apply
```

**Benefits:**
- No single point of failure
- Automatic leader election
- Split-brain prevention
- Eventual consistency guaranteed

### Real Infrastructure = Game State
No fake simulation:
```
Attack = Real UDP flood
Capacity = Real CPU allocation
Packet Loss = Real ACK measurement
Final Kill = Real WebSocket flood
```

**Benefits:**
- Fair (both players use same infrastructure)
- Educational (learn real networking)
- Unpredictable (actual network behavior)
- Fun (it's "real" hacking)

## Common Workflows

### Expanding Territory
1. Check neighbors: `GET /my/nodes`
2. Pick unoccupied hex
3. Attack it: `POST /my/attack`
4. Wait 5 seconds (overload duration)
5. Node captured, you own it
6. Repeat

### Defending
- You can't really defend
- Your only hope: have higher capacity (more CPU)
- Capital has 2x CPU → harder to overwhelm
- Spread out to avoid concentrated attacks

### Winning
1. Capture enemy capitals
2. Survive final kill attacks on your client
3. Eliminate all other players
4. Be last one standing

### Debugging

**Check if you're in a game:**
```bash
curl http://localhost:8080/status
```

**See your nodes:**
```bash
curl http://localhost:8080/my/nodes
```

**Watch real-time updates:**
```bash
websocat ws://localhost:8080/ws
```

**Check CloudWatch logs:**
```bash
aws logs tail /ecs/udp-nodes --follow
```

## Limitations & Constraints

### Infrastructure
- **AWS only:** Requires ECS, ECR, VPC
- **Region:** All in same AWS region
- **Network:** All in same VPC/subnet
- **Cost:** ECS Fargate is not free

### Game Design
- **No persistence:** Game state lost if all workers die
- **No replay:** Unless you save Raft log
- **No spectators:** Must join to watch
- **No pausing:** Game runs continuously

### Technical
- **Raft overhead:** ~50ms latency on events
- **UDP unreliability:** Packets can be lost (that's the point!)
- **WebSocket limits:** Browser connection limits for final kill
- **No authentication:** Anyone can join any game

### Scalability
- **Max players:** ~10-20 (Raft cluster size)
- **Max nodes:** 100s (limited by AWS quotas)
- **Max games:** 10s on one master (in-memory registry)

## Security Warning

**This is a CTF/educational tool, NOT production software.**

- No input validation
- No authentication
- No rate limiting
- No encryption
- Open UDP ports (DDoS vector)
- Direct infrastructure control

**Do NOT:**
- Run on public internet
- Use with untrusted players  
- Deploy to production environments
- Give it real credentials

**DO:**
- Run in isolated VPC
- Use for learning/competitions
- Add authentication if needed
- Monitor AWS costs

## Troubleshooting

### "No peer found" on join
- You're the first player - this is normal
- Your client bootstraps new Raft cluster

### Workers not spawning
- Check master logs: `aws logs tail /ecs/master --follow`
- Verify task definition registered
- Check ECS cluster exists
- Verify IAM permissions

### Can't attack neighbor
- Must own adjacent node
- Check coordinates: `curl localhost:8080/my/nodes`
- Hex grid, not square grid

### Node not capturing
- Need 20% packet loss for 5 seconds
- Check CloudWatch for metrics
- Verify leader is running game logic
- Capital has 2x capacity (harder to overload)

### Final kill not working
- Verify client exposes port 8080
- Check security group allows inbound TCP
- Look for "[FinalKill]" in client logs
- Connections should last 10 seconds

### Game stuck
- Check if leader exists: look for game logic logs
- Raft might be re-electing
- Check network connectivity between workers

## Further Reading

- `/master/CLAUDE.md` - Master server details
- `/worker/CLAUDE.md` - Worker node architecture (18 KB!)
- `/client/CLAUDE.md` - Client API & player lifecycle
- [OpenRaft docs](https://docs.rs/openraft) - Consensus algorithm
- [Tokio docs](https://tokio.rs) - Async runtime
- [Axum docs](https://docs.rs/axum) - HTTP framework

## Contributing

This is an educational project. Potential improvements:

- **Persistence:** Save Raft snapshots to S3
- **Replay:** Record and replay games
- **UI:** Web dashboard for game visualization
- **Bots:** AI players
- **Tournament mode:** Automated brackets
- **More attack types:** TCP SYN flood, HTTP flood, etc.
- **Defense mechanisms:** Rate limiting, traffic shaping
- **Multi-region:** Cross-region latency effects

## License

See LICENSE file.

---

**Built with Rust, OpenRaft, Tokio, Axum, and AWS ECS.**

*A distributed systems learning project disguised as a game.*
