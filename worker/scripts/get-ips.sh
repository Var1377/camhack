#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"

echo "Getting IP addresses of running UDP nodes..."

# Get all running task ARNs
TASK_ARNS=$(aws ecs list-tasks \
  --cluster $CLUSTER_NAME \
  --desired-status RUNNING \
  --region $AWS_REGION \
  --query 'taskArns[]' \
  --output text)

if [ -z "$TASK_ARNS" ]; then
  echo "No running tasks found in cluster $CLUSTER_NAME"
  exit 0
fi

# Get task details
TASKS=$(aws ecs describe-tasks \
  --cluster $CLUSTER_NAME \
  --tasks $TASK_ARNS \
  --region $AWS_REGION)

# Extract ENI IDs
ENI_IDS=$(echo "$TASKS" | jq -r '.tasks[].attachments[].details[] | select(.name=="networkInterfaceId") | .value')

echo "Found $(echo "$ENI_IDS" | wc -l) running nodes:"
echo ""
echo "NODE_ID | PUBLIC_IP | PRIVATE_IP"
echo "--------|-----------|------------"

# Get IP addresses for each ENI
for ENI_ID in $ENI_IDS; do
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

  NODE_NUM=$((NODE_NUM + 1))
  echo "node-$NODE_NUM | $PUBLIC_IP | $PRIVATE_IP"
done

echo ""
echo "To test UDP reflection, run:"
echo "  echo 'test' | nc -u PUBLIC_IP 8080"
echo ""
echo "To get stats from a node:"
echo "  echo 'stats' | nc PUBLIC_IP 8081"
echo ""
echo "To shutdown a node:"
echo "  echo 'shutdown' | nc PUBLIC_IP 8081"
