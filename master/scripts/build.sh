#!/bin/bash
set -e

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
ECR_REPO_NAME="master-node"
IMAGE_TAG="${IMAGE_TAG:-latest}"

echo "Building and pushing Master node Docker image..."
echo "AWS Account: $AWS_ACCOUNT_ID"
echo "Region: $AWS_REGION"
echo "ECR Repo: $ECR_REPO_NAME"
echo "Image Tag: $IMAGE_TAG"

# Create ECR repository if it doesn't exist
echo "Creating ECR repository (if needed)..."
aws ecr create-repository \
  --repository-name $ECR_REPO_NAME \
  --region $AWS_REGION 2>/dev/null || echo "Repository already exists"

# Login to ECR
echo "Logging in to ECR..."
aws ecr get-login-password --region $AWS_REGION | \
  docker login --username AWS --password-stdin \
  $AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com

# Build Docker image
echo "Building Docker image..."
cd "$(dirname "$0")/.."
docker build -t $ECR_REPO_NAME:$IMAGE_TAG .

# Tag image for ECR
ECR_IMAGE="$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/$ECR_REPO_NAME:$IMAGE_TAG"
docker tag $ECR_REPO_NAME:$IMAGE_TAG $ECR_IMAGE

# Push to ECR
echo "Pushing image to ECR..."
docker push $ECR_IMAGE

echo "✓ Image pushed successfully: $ECR_IMAGE"
