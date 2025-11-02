#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"

echo "Deploying Master node to ECS Fargate..."
echo "Cluster: $CLUSTER_NAME"
echo "Region: $AWS_REGION"

# Create ECS cluster if it doesn't exist
echo "Creating ECS cluster (if needed)..."
aws ecs create-cluster \
  --cluster-name $CLUSTER_NAME \
  --region $AWS_REGION 2>/dev/null || echo "Cluster already exists"

# Get default VPC and subnet
echo "Getting default subnet..."
SUBNET_ID=$(aws ec2 describe-subnets \
  --filters "Name=default-for-az,Values=true" \
  --region $AWS_REGION \
  --query 'Subnets[0].SubnetId' \
  --output text)
echo "Using subnet: $SUBNET_ID"

# Get default security group
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

# Add HTTP ingress rule if needed
echo "Ensuring TCP port 8080 is open..."
aws ec2 authorize-security-group-ingress \
  --group-id $SECURITY_GROUP_ID \
  --protocol tcp \
  --port 8080 \
  --cidr 0.0.0.0/0 \
  --region $AWS_REGION 2>/dev/null || echo "Rule already exists"

# Create or update IAM task role for master (needs ECS permissions)
echo "Creating IAM task role for master..."
TASK_ROLE_NAME="ecsMasterTaskRole"

# Create trust policy
cat > /tmp/ecs-task-trust-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Service": "ecs-tasks.amazonaws.com"
      },
      "Action": "sts:AssumeRole"
    }
  ]
}
EOF

# Create role (ignore if exists)
aws iam create-role \
  --role-name $TASK_ROLE_NAME \
  --assume-role-policy-document file:///tmp/ecs-task-trust-policy.json \
  --region $AWS_REGION 2>/dev/null || echo "Role already exists"

# Create policy for ECS operations
cat > /tmp/ecs-master-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "ecs:RunTask",
        "ecs:StopTask",
        "ecs:DescribeTasks",
        "ecs:ListTasks"
      ],
      "Resource": "*"
    },
    {
      "Effect": "Allow",
      "Action": [
        "iam:PassRole"
      ],
      "Resource": "*"
    }
  ]
}
EOF

# Attach policy to role
aws iam put-role-policy \
  --role-name $TASK_ROLE_NAME \
  --policy-name ECSMasterPolicy \
  --policy-document file:///tmp/ecs-master-policy.json \
  --region $AWS_REGION

echo "IAM role configured: $TASK_ROLE_NAME"

# Update task definition with correct values
echo "Updating task definition..."
cd "$(dirname "$0")/.."
sed "s/YOUR_ACCOUNT_ID/$AWS_ACCOUNT_ID/g" task-definition.json | \
  sed "s/us-east-1/$AWS_REGION/g" | \
  sed "s/WILL_BE_SET_BY_DEPLOY_SCRIPT/$SUBNET_ID/" | \
  sed "s/WILL_BE_SET_BY_DEPLOY_SCRIPT/$SECURITY_GROUP_ID/" > /tmp/master-task-definition-updated.json

# Also set SUBNET_ID and SECURITY_GROUP_ID in environment
python3 -c "
import json
import sys

with open('/tmp/master-task-definition-updated.json', 'r') as f:
    task_def = json.load(f)

# Update environment variables
for container in task_def['containerDefinitions']:
    for env in container['environment']:
        if env['name'] == 'SUBNET_ID':
            env['value'] = '$SUBNET_ID'
        elif env['name'] == 'SECURITY_GROUP_ID':
            env['value'] = '$SECURITY_GROUP_ID'

with open('/tmp/master-task-definition-updated.json', 'w') as f:
    json.dump(task_def, f, indent=2)
"

# Register task definition
echo "Registering task definition..."
TASK_DEF_ARN=$(aws ecs register-task-definition \
  --cli-input-json file:///tmp/master-task-definition-updated.json \
  --region $AWS_REGION \
  --query 'taskDefinition.taskDefinitionArn' \
  --output text)

echo "Task definition registered: $TASK_DEF_ARN"

# Check if master is already running
EXISTING_TASKS=$(aws ecs list-tasks \
  --cluster $CLUSTER_NAME \
  --family master-node \
  --desired-status RUNNING \
  --region $AWS_REGION \
  --query 'taskArns[]' \
  --output text)

if [ -n "$EXISTING_TASKS" ]; then
  echo "⚠️  Master node is already running"
  echo "Task ARN: $EXISTING_TASKS"
  echo ""
  read -p "Stop and restart with new image? (y/N): " -n 1 -r
  echo ""
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Stopping existing master task..."
    aws ecs stop-task \
      --cluster $CLUSTER_NAME \
      --task $EXISTING_TASKS \
      --region $AWS_REGION \
      --output text > /dev/null

    echo "Waiting for task to stop..."
    sleep 10

    # Verify it stopped
    STILL_RUNNING=$(aws ecs list-tasks \
      --cluster $CLUSTER_NAME \
      --family master-node \
      --desired-status RUNNING \
      --region $AWS_REGION \
      --query 'taskArns[]' \
      --output text)

    if [ -n "$STILL_RUNNING" ]; then
      echo "⚠️  Task still stopping, waiting 10 more seconds..."
      sleep 10
    fi

    echo "✓ Old master stopped"
  else
    echo "Keeping existing master. Exiting."
    echo ""
    echo "To get the master IP:"
    echo "  ./scripts/get-ip.sh"
    exit 0
  fi
fi

# Run the master task
echo "Launching Master node..."
TASK_ARN=$(aws ecs run-task \
  --cluster $CLUSTER_NAME \
  --task-definition master-node \
  --count 1 \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[$SUBNET_ID],securityGroups=[$SECURITY_GROUP_ID],assignPublicIp=ENABLED}" \
  --region $AWS_REGION \
  --query 'tasks[0].taskArn' \
  --output text)

echo "✓ Master node launched successfully"
echo "Task ARN: $TASK_ARN"
echo ""
echo "Wait 30-60 seconds for the task to start, then run:"
echo "  ./scripts/get-ip.sh"
