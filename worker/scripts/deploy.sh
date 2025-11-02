#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"
TASK_COUNT="${TASK_COUNT:-1}"
SUBNET_ID="${SUBNET_ID:-}"
SECURITY_GROUP_ID="${SECURITY_GROUP_ID:-}"
MASTER_IP="${MASTER_IP:-}"

# Check for MASTER_IP - required for workers to register with master
if [ -z "$MASTER_IP" ]; then
  echo "ERROR: MASTER_IP environment variable not set"
  echo ""
  echo "Workers need to know the master's IP address to register."
  echo "Please run one of the following:"
  echo ""
  echo "  # Option 1: Get IP from running master"
  echo "  export MASTER_IP=\$(cd ../master && ./scripts/get-ip.sh | grep -oE '([0-9]{1,3}\.){3}[0-9]{1,3}' | head -1)"
  echo ""
  echo "  # Option 2: Use a specific IP"
  echo "  export MASTER_IP=54.123.45.67"
  echo ""
  echo "Then re-run this script."
  exit 1
fi

echo "Deploying UDP nodes to ECS Fargate..."
echo "Cluster: $CLUSTER_NAME"
echo "Task count: $TASK_COUNT"
echo "Region: $AWS_REGION"
echo "Master IP: $MASTER_IP"

# Create ECS cluster if it doesn't exist
echo "Creating ECS cluster (if needed)..."
aws ecs create-cluster \
  --cluster-name $CLUSTER_NAME \
  --region $AWS_REGION 2>/dev/null || echo "Cluster already exists"

# Update task definitions with correct account ID, region, and master IP
echo "Updating task definitions..."
cd "$(dirname "$0")/.."

# Process regular worker task definition
echo "  - Regular worker (udp-node)"
sed "s/YOUR_ACCOUNT_ID/$AWS_ACCOUNT_ID/g" task-definition.json | \
  sed "s/us-east-1/$AWS_REGION/g" | \
  sed "s|REPLACE_WITH_MASTER_IP|$MASTER_IP|g" > /tmp/task-definition-updated.json

# Process capital worker task definition
echo "  - Capital worker (udp-node-capital)"
sed "s/YOUR_ACCOUNT_ID/$AWS_ACCOUNT_ID/g" task-definition-capital.json | \
  sed "s/us-east-1/$AWS_REGION/g" | \
  sed "s|REPLACE_WITH_MASTER_IP|$MASTER_IP|g" > /tmp/task-definition-capital-updated.json

# Register both task definitions
echo "Registering regular worker task definition..."
TASK_DEF_ARN=$(aws ecs register-task-definition \
  --cli-input-json file:///tmp/task-definition-updated.json \
  --region $AWS_REGION \
  --query 'taskDefinition.taskDefinitionArn' \
  --output text)
echo "✓ Regular worker task definition registered: $TASK_DEF_ARN"

echo "Registering capital worker task definition..."
CAPITAL_TASK_DEF_ARN=$(aws ecs register-task-definition \
  --cli-input-json file:///tmp/task-definition-capital-updated.json \
  --region $AWS_REGION \
  --query 'taskDefinition.taskDefinitionArn' \
  --output text)
echo "✓ Capital worker task definition registered: $CAPITAL_TASK_DEF_ARN"

# Get default VPC and subnet if not provided
if [ -z "$SUBNET_ID" ]; then
  echo "Getting default subnet..."
  SUBNET_ID=$(aws ec2 describe-subnets \
    --filters "Name=default-for-az,Values=true" \
    --region $AWS_REGION \
    --query 'Subnets[0].SubnetId' \
    --output text)
  echo "Using subnet: $SUBNET_ID"
fi

if [ -z "$SECURITY_GROUP_ID" ]; then
  echo "Getting default security group..."
  VPC_ID=$(aws ec2 describe-subnets \
    --subnet-ids $SUBNET_ID \
    --region $AWS_REGION \
    --query 'Subnets[0].VpcId' \
    --output text)

  SECURITY_GROUP_ID=$(aws ec2 describe-security-groups \
    --filters "Name=vpc-id,Values=$VPC_ID" "Name=group-name,Values=default" \
    --region $AWS_REGION \
    --query 'SecurityGroups[0].GroupId' \
    --output text)

  echo "Using security group: $SECURITY_GROUP_ID"

  # Add UDP ingress rule if needed
  echo "Ensuring UDP port 8080 is open..."
  aws ec2 authorize-security-group-ingress \
    --group-id $SECURITY_GROUP_ID \
    --protocol udp \
    --port 8080 \
    --cidr 0.0.0.0/0 \
    --region $AWS_REGION 2>/dev/null || echo "Rule already exists"

  # Add TCP control port rule
  echo "Ensuring TCP port 8081 is open..."
  aws ec2 authorize-security-group-ingress \
    --group-id $SECURITY_GROUP_ID \
    --protocol tcp \
    --port 8081 \
    --cidr 0.0.0.0/0 \
    --region $AWS_REGION 2>/dev/null || echo "Rule already exists"
fi

# Run tasks
echo "Launching $TASK_COUNT Fargate tasks..."
aws ecs run-task \
  --cluster $CLUSTER_NAME \
  --task-definition udp-node \
  --count $TASK_COUNT \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[$SUBNET_ID],securityGroups=[$SECURITY_GROUP_ID],assignPublicIp=ENABLED}" \
  --region $AWS_REGION \
  --query 'tasks[].taskArn' \
  --output table

echo "✓ Tasks launched successfully"
echo ""
echo "Wait 30-60 seconds for tasks to start, then run:"
echo "  ./scripts/get-ips.sh"
