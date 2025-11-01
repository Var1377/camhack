#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"

echo "Getting Master node IP address..."

# Get master task ARN
TASK_ARN=$(aws ecs list-tasks \
  --cluster $CLUSTER_NAME \
  --family master-node \
  --desired-status RUNNING \
  --region $AWS_REGION \
  --query 'taskArns[0]' \
  --output text)

if [ -z "$TASK_ARN" ] || [ "$TASK_ARN" == "None" ]; then
  echo "No running master task found in cluster $CLUSTER_NAME"
  exit 1
fi

echo "Master task: $TASK_ARN"

# Get task details
TASK_DETAILS=$(aws ecs describe-tasks \
  --cluster $CLUSTER_NAME \
  --tasks $TASK_ARN \
  --region $AWS_REGION)

# Extract ENI ID
ENI_ID=$(echo "$TASK_DETAILS" | jq -r '.tasks[0].attachments[0].details[] | select(.name=="networkInterfaceId") | .value')

if [ -z "$ENI_ID" ]; then
  echo "Could not find network interface for master task"
  exit 1
fi

# Get IP addresses
IP_INFO=$(aws ec2 describe-network-interfaces \
  --network-interface-ids $ENI_ID \
  --region $AWS_REGION \
  --query 'NetworkInterfaces[0].[Association.PublicIp,PrivateIpAddress]' \
  --output text)

PUBLIC_IP=$(echo "$IP_INFO" | awk '{print $1}')
PRIVATE_IP=$(echo "$IP_INFO" | awk '{print $2}')

if [ "$PUBLIC_IP" == "None" ] || [ -z "$PUBLIC_IP" ]; then
  PUBLIC_IP="N/A"
fi

echo ""
echo "Master Node Status:"
echo "  Public IP:  $PUBLIC_IP"
echo "  Private IP: $PRIVATE_IP"
echo "  HTTP Port:  8080"
echo ""
echo "API Endpoints:"
echo "  Health:        curl http://$PUBLIC_IP:8080/"
echo "  Status:        curl http://$PUBLIC_IP:8080/status"
echo "  Spawn workers: curl -X POST 'http://$PUBLIC_IP:8080/spawn_workers?count=10'"
echo "  Kill workers:  curl -X POST http://$PUBLIC_IP:8080/kill_workers"
echo "  Kill master:   curl -X POST http://$PUBLIC_IP:8080/kill"
echo ""
