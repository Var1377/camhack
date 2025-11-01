#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"

echo "Tearing down UDP test cluster..."
echo "Cluster: $CLUSTER_NAME"
echo "Region: $AWS_REGION"

# Stop all running tasks
echo "Stopping all running tasks..."
TASK_ARNS=$(aws ecs list-tasks \
  --cluster $CLUSTER_NAME \
  --desired-status RUNNING \
  --region $AWS_REGION \
  --query 'taskArns[]' \
  --output text)

if [ -n "$TASK_ARNS" ]; then
  for TASK_ARN in $TASK_ARNS; do
    echo "Stopping task: $TASK_ARN"
    aws ecs stop-task \
      --cluster $CLUSTER_NAME \
      --task $TASK_ARN \
      --region $AWS_REGION \
      --output text > /dev/null
  done
  echo "✓ All tasks stopped"
else
  echo "No running tasks found"
fi

# Optional: Delete the cluster
read -p "Delete the ECS cluster? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
  echo "Deleting cluster..."
  aws ecs delete-cluster \
    --cluster $CLUSTER_NAME \
    --region $AWS_REGION \
    --output text > /dev/null
  echo "✓ Cluster deleted"
fi

# Optional: Delete task definitions
read -p "Deregister all task definitions? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
  echo "Deregistering task definitions..."
  TASK_DEFS=$(aws ecs list-task-definitions \
    --family-prefix udp-node \
    --region $AWS_REGION \
    --query 'taskDefinitionArns[]' \
    --output text)

  for TASK_DEF in $TASK_DEFS; do
    aws ecs deregister-task-definition \
      --task-definition $TASK_DEF \
      --region $AWS_REGION \
      --output text > /dev/null
  done
  echo "✓ Task definitions deregistered"
fi

echo ""
echo "✓ Teardown complete"
echo ""
echo "Note: ECR repository and CloudWatch logs were NOT deleted."
echo "To delete ECR repository:"
echo "  aws ecr delete-repository --repository-name udp-node --force --region $AWS_REGION"
echo "To delete CloudWatch logs:"
echo "  aws logs delete-log-group --log-group-name /ecs/udp-nodes --region $AWS_REGION"
