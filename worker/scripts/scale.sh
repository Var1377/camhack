#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"
TARGET_COUNT="${1:-10}"

if [ -z "$1" ]; then
  echo "Usage: $0 <target_node_count>"
  echo "Example: $0 100  # Scale to 100 nodes"
  exit 1
fi

echo "Scaling cluster to $TARGET_COUNT nodes..."

# Get current task count
CURRENT_TASKS=$(aws ecs list-tasks \
  --cluster $CLUSTER_NAME \
  --desired-status RUNNING \
  --region $AWS_REGION \
  --query 'taskArns' \
  --output text | wc -w)

echo "Current running tasks: $CURRENT_TASKS"

if [ "$TARGET_COUNT" -gt "$CURRENT_TASKS" ]; then
  # Scale up
  SCALE_UP=$((TARGET_COUNT - CURRENT_TASKS))
  echo "Scaling up by $SCALE_UP tasks..."

  # Get network config from existing tasks
  TASK_ARN=$(aws ecs list-tasks \
    --cluster $CLUSTER_NAME \
    --desired-status RUNNING \
    --region $AWS_REGION \
    --query 'taskArns[0]' \
    --output text)

  if [ -z "$TASK_ARN" ] || [ "$TASK_ARN" == "None" ]; then
    echo "Error: No running tasks to copy network config from"
    echo "Run ./scripts/deploy.sh first"
    exit 1
  fi

  # Get subnet and security group from existing task
  TASK_DETAILS=$(aws ecs describe-tasks \
    --cluster $CLUSTER_NAME \
    --tasks $TASK_ARN \
    --region $AWS_REGION)

  SUBNET_ID=$(echo "$TASK_DETAILS" | jq -r '.tasks[0].attachments[0].details[] | select(.name=="subnetId") | .value')

  # Get security group from ENI
  ENI_ID=$(echo "$TASK_DETAILS" | jq -r '.tasks[0].attachments[0].details[] | select(.name=="networkInterfaceId") | .value')
  SECURITY_GROUP_ID=$(aws ec2 describe-network-interfaces \
    --network-interface-ids $ENI_ID \
    --region $AWS_REGION \
    --query 'NetworkInterfaces[0].Groups[0].GroupId' \
    --output text)

  # Launch new tasks
  aws ecs run-task \
    --cluster $CLUSTER_NAME \
    --task-definition udp-node \
    --count $SCALE_UP \
    --launch-type FARGATE \
    --network-configuration "awsvpcConfiguration={subnets=[$SUBNET_ID],securityGroups=[$SECURITY_GROUP_ID],assignPublicIp=ENABLED}" \
    --region $AWS_REGION \
    --query 'tasks[].taskArn' \
    --output table

  echo "✓ Launched $SCALE_UP new tasks"

elif [ "$TARGET_COUNT" -lt "$CURRENT_TASKS" ]; then
  # Scale down
  SCALE_DOWN=$((CURRENT_TASKS - TARGET_COUNT))
  echo "Scaling down by $SCALE_DOWN tasks..."

  # Get task ARNs to stop
  TASKS_TO_STOP=$(aws ecs list-tasks \
    --cluster $CLUSTER_NAME \
    --desired-status RUNNING \
    --region $AWS_REGION \
    --query "taskArns[:$SCALE_DOWN]" \
    --output text)

  for TASK_ARN in $TASKS_TO_STOP; do
    aws ecs stop-task \
      --cluster $CLUSTER_NAME \
      --task $TASK_ARN \
      --region $AWS_REGION \
      --output text > /dev/null
  done

  echo "✓ Stopped $SCALE_DOWN tasks"
else
  echo "Already at target count"
fi

echo ""
echo "Cluster scaled to $TARGET_COUNT nodes (may take 30-60 seconds to fully start)"
echo "Run ./scripts/get-ips.sh to see all node IPs"
