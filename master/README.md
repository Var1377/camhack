# Master Node - Worker Orchestration Control Plane

The Master node is a long-running HTTP API server that orchestrates worker lifecycle on AWS ECS Fargate.

## Features

- **Globally unique** - Only one master runs at a time
- **Long-running** - Stays alive indefinitely until explicitly killed
- **HTTP API** - Simple REST endpoints to control workers
- **ECS Integration** - Spawns and kills workers using AWS ECS API

## Architecture

```
┌─────────────────────────────────────────────┐
│ Master Node (Single Fargate Task)           │
│                                             │
│  HTTP API (Port 8080)                       │
│  ├─ POST /spawn_workers?count=N            │
│  ├─ POST /kill_workers                      │
│  ├─ POST /kill                              │
│  └─ GET  /status                            │
│                                             │
│  Uses AWS SDK to:                           │
│  - Run ECS tasks (spawn workers)            │
│  - Stop ECS tasks (kill workers)            │
└─────────────────────────────────────────────┘
           │
           │ ECS API Calls
           ↓
┌─────────────────────────────────────────────┐
│ Worker Nodes (Multiple Fargate Tasks)       │
│  - Worker 1                                 │
│  - Worker 2                                 │
│  - Worker 3                                 │
│  - ...                                      │
└─────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- AWS CLI configured
- Docker installed
- Worker node already built and pushed to ECR

### 1. Build Master Image

```bash
cd master
chmod +x scripts/*.sh
./scripts/build.sh
```

### 2. Deploy Master

```bash
./scripts/deploy.sh
```

Wait 30-60 seconds for the master to start.

### 3. Get Master IP

```bash
./scripts/get-ip.sh
```

Output:
```
Master Node Status:
  Public IP:  34.123.45.67
  Private IP: 10.0.1.23
  HTTP Port:  8080

API Endpoints:
  Health:        curl http://34.123.45.67:8080/
  Status:        curl http://34.123.45.67:8080/status
  Spawn workers: curl -X POST 'http://34.123.45.67:8080/spawn_workers?count=10'
  Kill workers:  curl -X POST http://34.123.45.67:8080/kill_workers
  Kill master:   curl -X POST http://34.123.45.67:8080/kill
```

## API Reference

### Health Check

```bash
curl http://MASTER_IP:8080/
```

Response:
```
Master node is alive
```

### Get Status

```bash
curl http://MASTER_IP:8080/status
```

Response:
```json
{
  "status": "running",
  "active_workers": 5,
  "worker_tasks": [
    "arn:aws:ecs:us-east-1:123456789:task/...",
    "arn:aws:ecs:us-east-1:123456789:task/...",
    ...
  ]
}
```

### Spawn Workers

```bash
curl -X POST 'http://MASTER_IP:8080/spawn_workers?count=10'
```

Response:
```json
{
  "message": "Successfully spawned 10 workers",
  "spawned_count": 10,
  "task_arns": [
    "arn:aws:ecs:us-east-1:123456789:task/...",
    ...
  ]
}
```

### Kill Workers

```bash
curl -X POST http://MASTER_IP:8080/kill_workers
```

Response:
```json
{
  "message": "Killed 10 workers",
  "killed_count": 10
}
```

### Kill Master (Self-Terminate)

```bash
curl -X POST http://MASTER_IP:8080/kill
```

Response:
```
Master terminating...
```

The master task will stop and container will exit.

## Configuration

The master is configured via environment variables in `task-definition.json`:

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | HTTP server port | `8080` |
| `CLUSTER_NAME` | ECS cluster name | `udp-test-cluster` |
| `WORKER_TASK_DEFINITION` | Worker task definition family | `worker` |
| `SUBNET_ID` | Subnet for spawned workers | Set by deploy script |
| `SECURITY_GROUP_ID` | Security group for workers | Set by deploy script |

## IAM Permissions

The master task role (`ecsMasterTaskRole`) needs:

```json
{
  "Effect": "Allow",
  "Action": [
    "ecs:RunTask",
    "ecs:StopTask",
    "ecs:DescribeTasks",
    "ecs:ListTasks",
    "iam:PassRole"
  ],
  "Resource": "*"
}
```

This is automatically created by `deploy.sh`.

## Lifecycle

### Deploy Master

```bash
./scripts/deploy.sh
```

Master starts and runs indefinitely.

### Spawn Workers

```bash
MASTER_IP=$(./scripts/get-ip.sh | grep "Public IP" | awk '{print $3}')
curl -X POST "http://$MASTER_IP:8080/spawn_workers?count=50"
```

### Check Status

```bash
curl http://$MASTER_IP:8080/status | jq
```

### Kill Workers

```bash
curl -X POST http://$MASTER_IP:8080/kill_workers
```

### Kill Master

```bash
curl -X POST http://$MASTER_IP:8080/kill
```

Or stop manually:
```bash
TASK_ARN=$(aws ecs list-tasks --cluster udp-test-cluster --family master-node --query 'taskArns[0]' --output text)
aws ecs stop-task --cluster udp-test-cluster --task $TASK_ARN
```

## Monitoring

### CloudWatch Logs

```bash
aws logs tail /ecs/master-node --follow
```

### ECS Console

```
https://console.aws.amazon.com/ecs/home?region=us-east-1#/clusters/udp-test-cluster/tasks
```

## Troubleshooting

### Master won't start

Check CloudWatch logs:
```bash
aws logs tail /ecs/master-node --follow
```

Common issues:
- Missing `SUBNET_ID` or `SECURITY_GROUP_ID` environment variables
- IAM role not created or missing permissions
- Security group doesn't allow port 8080

### Can't spawn workers

Ensure:
- Worker task definition exists (`worker`)
- Worker Docker image is pushed to ECR
- IAM task role has `ecs:RunTask` permission
- Subnet and security group are valid

### Multiple masters running

Only one master should run. Check:
```bash
aws ecs list-tasks --cluster udp-test-cluster --family master-node
```

If multiple exist, stop extras:
```bash
aws ecs stop-task --cluster udp-test-cluster --task TASK_ARN
```

## Development

### Local Testing (without AWS)

The master requires AWS credentials and ECS access. For local development:

1. Mock the ECS client
2. Use environment variables for testing
3. Test API endpoints with curl

### Building Locally

```bash
cd master
cargo build --release
```

Run with mock AWS config:
```bash
SUBNET_ID=subnet-test \
SECURITY_GROUP_ID=sg-test \
cargo run
```

## Files

```
master/
├── src/main.rs              # HTTP API + ECS integration
├── Cargo.toml               # Rust dependencies
├── Dockerfile               # Multi-stage build
├── task-definition.json     # ECS Fargate task config
├── scripts/
│   ├── build.sh            # Build and push image
│   ├── deploy.sh           # Deploy master task
│   └── get-ip.sh           # Get master IP
└── README.md               # This file
```

## Cost

Master node (0.25 vCPU, 512 MB):
- ~$0.012 per hour
- ~$8.64 per month (running 24/7)

For testing, start/stop as needed to minimize cost.

## Security Notes

- Master has public IP by default (for easy access)
- No authentication on API endpoints
- IAM role can spawn unlimited workers
- For production: Use ALB + private subnet + authentication

## License

MIT
