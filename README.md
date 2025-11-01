# CamHack - Master-Worker Control System

A distributed control system for spawning and managing worker nodes on AWS ECS Fargate.

## Overview

This project consists of two components:

1. **Master Node** (`master/`) - A long-running, globally unique control plane
2. **Worker Nodes** (`worker/`) - Ephemeral workers spawned on demand

## Architecture

```
                           HTTP Requests
    Client ──────────────────────────────────────▶ Master Node
                                                   (Fargate Task)
                                                         │
                                                         │ AWS ECS API
                                                         ├─ run_task()
                                                         └─ stop_task()
                                                         │
                                                         ↓
                                              ┌──────────────────────┐
                                              │  Worker Nodes        │
                                              │  ┌────────────────┐  │
                                              │  │ Worker 1       │  │
                                              │  └────────────────┘  │
                                              │  ┌────────────────┐  │
                                              │  │ Worker 2       │  │
                                              │  └────────────────┘  │
                                              │  ┌────────────────┐  │
                                              │  │ Worker N       │  │
                                              │  └────────────────┘  │
                                              └──────────────────────┘
```

## Components

### Master Node

- **HTTP API server** running on port 8080
- **Globally unique** - only one master exists
- **Long-running** - stays alive until explicitly killed
- **Endpoints**:
  - `POST /spawn_workers?count=N` - Spawn N workers
  - `POST /kill_workers` - Kill all spawned workers
  - `POST /kill` - Kill the master itself
  - `GET /status` - Get status and active worker count

See `master/README.md` for details.

### Worker Nodes

- **Minimal placeholder** - currently just logs heartbeats
- **Ephemeral** - spawned and killed by master on demand
- **Scalable** - can spawn 100+ workers in parallel

See `worker/README.md` for details.

## Quick Start

### 1. Build Both Images

```bash
# Build worker
cd worker
./scripts/build.sh

# Build master
cd ../master
./scripts/build.sh
```

### 2. Deploy Worker Task Definition

```bash
cd worker
./scripts/deploy.sh
```

This creates the worker task definition (but doesn't spawn any workers yet).

### 3. Deploy Master

```bash
cd master
./scripts/deploy.sh
```

Wait 30-60 seconds for master to start.

### 4. Get Master IP

```bash
./scripts/get-ip.sh
```

### 5. Use the Master API

```bash
MASTER_IP=<IP_from_previous_step>

# Spawn 10 workers
curl -X POST "http://$MASTER_IP:8080/spawn_workers?count=10"

# Check status
curl http://$MASTER_IP:8080/status

# Kill workers
curl -X POST http://$MASTER_IP:8080/kill_workers

# Kill master
curl -X POST http://$MASTER_IP:8080/kill
```

## Project Structure

```
camhack/
├── master/                    # Master node
│   ├── src/main.rs           # HTTP API + ECS integration
│   ├── Cargo.toml            # Dependencies (axum, aws-sdk-ecs)
│   ├── Dockerfile            # Container build
│   ├── task-definition.json  # ECS Fargate config
│   ├── scripts/
│   │   ├── build.sh         # Build and push image
│   │   ├── deploy.sh        # Deploy master
│   │   └── get-ip.sh        # Get master IP
│   └── README.md
│
├── worker/                    # Worker nodes
│   ├── src/main.rs           # Worker implementation
│   ├── Cargo.toml            # Dependencies
│   ├── Dockerfile            # Container build
│   ├── task-definition.json  # ECS Fargate config
│   ├── scripts/
│   │   ├── build.sh         # Build and push image
│   │   └── deploy.sh        # Deploy task definition
│   └── README.md
│
└── README.md                 # This file
```

## How It Works

1. **Master starts** as a single Fargate task with a public IP
2. **Client sends HTTP request** to master: `POST /spawn_workers?count=50`
3. **Master calls AWS ECS API** to run 50 worker tasks
4. **Workers start** as independent Fargate tasks
5. **Master tracks** worker task ARNs in memory
6. **Client requests kill**: `POST /kill_workers`
7. **Master stops** all tracked worker tasks via ECS API

## Use Cases

- **DDoS testing** - Spawn 100+ workers to flood a target
- **Distributed computing** - Coordinate work across many nodes
- **Load testing** - Spin up workers on demand, tear down when done
- **Research** - Experiment with distributed systems

## Configuration

### Master Environment Variables

Set in `master/task-definition.json`:
- `CLUSTER_NAME` - ECS cluster (default: `udp-test-cluster`)
- `WORKER_TASK_DEFINITION` - Worker task definition family (default: `worker`)
- `SUBNET_ID` - Subnet for workers (set by deploy script)
- `SECURITY_GROUP_ID` - Security group for workers (set by deploy script)

### Worker Environment Variables

Set in `worker/task-definition.json`:
- `WORKER_ID` - Unique identifier (auto-generated if not set)

## Cost Estimate

Fargate pricing (0.25 vCPU, 512 MB per task):

| Component | Runtime | Cost |
|-----------|---------|------|
| Master (24/7) | 720 hours/month | ~$8.64/month |
| 50 workers for 2 hours | 100 task-hours | ~$1.20 |
| 100 workers for 15 min | 25 task-hours | ~$0.30 |

**Tip**: Stop master when not in use to save cost.

## Monitoring

### CloudWatch Logs

```bash
# Master logs
aws logs tail /ecs/master-node --follow

# Worker logs
aws logs tail /ecs/worker --follow
```

### ECS Console

```
https://console.aws.amazon.com/ecs/home?region=us-east-1#/clusters/udp-test-cluster/tasks
```

## Development

### Local Testing

Both components can be built and tested locally:

```bash
# Build locally
cd master
cargo build --release

cd ../worker
cargo build --release
```

For master, you'll need AWS credentials and valid environment variables.

## Cleanup

### Stop All Workers

```bash
curl -X POST http://$MASTER_IP:8080/kill_workers
```

### Stop Master

```bash
curl -X POST http://$MASTER_IP:8080/kill
```

### Manual Cleanup

```bash
# Stop all tasks
aws ecs list-tasks --cluster udp-test-cluster --query 'taskArns[]' --output text | \
  xargs -I {} aws ecs stop-task --cluster udp-test-cluster --task {}

# Delete cluster
aws ecs delete-cluster --cluster udp-test-cluster

# Delete ECR repositories
aws ecr delete-repository --repository-name master-node --force
aws ecr delete-repository --repository-name udp-node --force
```

## Security Notes

- **Public IPs** - Master and workers have public IPs by default
- **No authentication** - API endpoints are open (add auth for production)
- **IAM permissions** - Master can spawn unlimited workers
- **Security groups** - Default SG is open (restrict for production)

## Next Steps

1. **Implement worker logic** - Currently workers just log heartbeats
2. **Add authentication** - Protect master API endpoints
3. **Add metrics** - Track worker performance and status
4. **Inter-worker communication** - Allow workers to talk to each other
5. **State management** - Add persistence for worker tracking

## License

MIT
