# UDP Node - ECS Fargate Deployment

Fast-scaling UDP reflection nodes for distributed testing, DDoS simulation, and packet loss measurement.

## Features

- **UDP packet reflection** - Echoes back any UDP packet with metadata
- **Packet tracking** - Sequence numbers and timestamps for loss detection
- **Control interface** - TCP port for stats and shutdown commands
- **Fast scaling** - Deploy 100+ nodes in 30-60 seconds
- **Complete teardown** - No idle costs between tests

## Architecture

Each node runs:
- **UDP server** (port 8080): Reflects packets back to sender with metadata
- **Control server** (port 8081): TCP interface for stats and shutdown

Deployed on **AWS ECS Fargate** with awsvpc networking (each node gets its own IP).

## Quick Start

### Prerequisites

- AWS CLI configured with credentials
- Docker installed
- `jq` installed for IP parsing

### 1. Build and Push Docker Image

```bash
cd worker
chmod +x scripts/*.sh
./scripts/build.sh
```

This builds the Docker image and pushes it to Amazon ECR.

### 2. Deploy Initial Cluster

```bash
# Deploy 1 node to start
./scripts/deploy.sh

# Or deploy multiple nodes
TASK_COUNT=10 ./scripts/deploy.sh
```

Wait 30-60 seconds for tasks to start.

### 3. Get Node IPs

```bash
./scripts/get-ips.sh
```

Output:
```
NODE_ID  | PUBLIC_IP      | PRIVATE_IP
---------|----------------|------------
node-1   | 34.123.45.67   | 10.0.1.23
node-2   | 54.234.56.78   | 10.0.1.45
...
```

### 4. Test UDP Reflection

```bash
# Send a test packet
echo "hello" | nc -u 34.123.45.67 8080

# Response (JSON):
# {"seq":0,"timestamp":1234567890,"payload":[104,101,108,108,111],"node_id":"node-1"}
```

### 5. Scale the Cluster

```bash
# Scale to 100 nodes
./scripts/scale.sh 100

# Scale down to 10 nodes
./scripts/scale.sh 10
```

### 6. Teardown

```bash
./scripts/teardown.sh
```

## Usage Examples

### UDP Packet Reflection Test

```bash
# Get a node IP
NODE_IP=$(./scripts/get-ips.sh | grep node-1 | awk '{print $3}')

# Send UDP packet
echo "test packet" | nc -u $NODE_IP 8080
```

### Get Node Statistics

```bash
# Connect to control port and request stats
echo "stats" | nc $NODE_IP 8081

# Output:
# Packets: RX=42 TX=42
# Bytes: RX=1234 TX=5678
```

### Shutdown a Node

```bash
echo "shutdown" | nc $NODE_IP 8081
```

### Measure Packet Loss

```python
import socket
import json
import time

# Send 1000 packets
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.settimeout(1.0)

sent = 0
received = 0

for i in range(1000):
    payload = json.dumps({"seq": i, "data": "test"})
    sock.sendto(payload.encode(), (NODE_IP, 8080))
    sent += 1

    try:
        data, addr = sock.recvfrom(65535)
        received += 1
    except socket.timeout:
        pass

packet_loss = ((sent - received) / sent) * 100
print(f"Packet loss: {packet_loss:.2f}%")
```

### DDoS Testing (Node-to-Node)

```bash
# Deploy 100 nodes
./scripts/scale.sh 100

# Get all IPs
IPS=$(./scripts/get-ips.sh | grep node | awk '{print $3}')

# From each node, send packets to all other nodes
# (This requires custom client code or scripts)
```

## Configuration

### Environment Variables

Set these before running scripts:

```bash
export AWS_REGION=us-west-2           # AWS region
export CLUSTER_NAME=my-udp-cluster    # ECS cluster name
export TASK_COUNT=50                  # Initial node count
export IMAGE_TAG=v1.0                 # Docker image tag
```

### Node Configuration

Edit `task-definition.json` to change:
- CPU/Memory (default: 0.25 vCPU, 512 MB)
- Environment variables (UDP_PORT, CONTROL_PORT, NODE_ID)

### Network Configuration

The deploy script uses your default VPC and subnet. To use custom networking:

```bash
SUBNET_ID=subnet-abc123 \
SECURITY_GROUP_ID=sg-xyz789 \
./scripts/deploy.sh
```

## Performance

### Startup Times

- **First deployment**: 60-90 seconds (image pull + task start)
- **Subsequent scaling**: 30-60 seconds (cached image)
- **Parallel scaling**: All tasks start simultaneously

### Scaling Limits

- **Fargate default**: 100 tasks per run-task call
- **Account limit**: 1000+ concurrent tasks (varies by region)
- **For 100+ nodes**: Script handles automatically

### Cost Estimate

Fargate pricing (0.25 vCPU, 512 MB):
- ~$0.012 per node per hour
- 100 nodes for 2 hours: **$2.40**
- 100 nodes for 15 minutes: **$0.60**

## Monitoring

### CloudWatch Logs

```bash
# Tail logs for all nodes
aws logs tail /ecs/udp-nodes --follow

# View specific node
aws logs tail /ecs/udp-nodes --follow --filter-pattern "node-1"
```

### ECS Console

View tasks and metrics:
```
https://console.aws.amazon.com/ecs/home?region=us-east-1#/clusters/udp-test-cluster/tasks
```

## Troubleshooting

### Tasks fail to start

Check security group allows:
- UDP port 8080 inbound
- TCP port 8081 inbound

```bash
# Add rules manually
aws ec2 authorize-security-group-ingress \
  --group-id sg-xxx \
  --protocol udp \
  --port 8080 \
  --cidr 0.0.0.0/0
```

### Can't connect to nodes

Wait 30-60 seconds after deployment, then verify:
```bash
./scripts/get-ips.sh
nc -vz PUBLIC_IP 8081  # Test control port
```

### Image pull errors

Ensure ECR repository exists and image is pushed:
```bash
aws ecr describe-repositories --repository-names udp-node
aws ecr describe-images --repository-name udp-node
```

## Advanced Usage

### Custom Packet Format

Modify `src/main.rs` to change packet structure or add custom logic.

### Load Testing

Use tools like `iperf3` or custom UDP flood scripts:
```bash
# Install on a node (requires EC2 approach, not Fargate)
# Or use external load generation
```

### Multi-Region Deployment

Deploy to multiple regions:
```bash
AWS_REGION=us-west-2 ./scripts/deploy.sh
AWS_REGION=eu-west-1 ./scripts/deploy.sh
```

## Files

```
worker/
├── src/main.rs                 # UDP node implementation
├── Cargo.toml                  # Rust dependencies
├── Dockerfile                  # Multi-stage build
├── task-definition.json        # ECS Fargate task config
├── scripts/
│   ├── build.sh               # Build and push image
│   ├── deploy.sh              # Deploy cluster
│   ├── scale.sh               # Scale nodes up/down
│   ├── get-ips.sh             # List node IPs
│   └── teardown.sh            # Destroy cluster
└── README.md                  # This file
```

## Security Notes

- Nodes are deployed with **public IPs** for easy testing
- Security group is **open to 0.0.0.0/0** by default
- For production, use private subnets and restrict access
- Control port (8081) allows anyone to shutdown nodes

## License

MIT
