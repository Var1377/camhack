#!/bin/bash
set -e

# CamHack - Global Build and Deploy Script
# Non-interactive: automatically kills existing instances and deploys

echo "=========================================="
echo "CamHack - Global Build & Deploy"
echo "=========================================="
echo ""

# Configuration
AWS_REGION="${AWS_REGION:-us-east-1}"
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
CLUSTER_NAME="${CLUSTER_NAME:-udp-test-cluster}"
REPO_ROOT="/root/camhack"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# ====================
# Phase 0: Check for existing MASTER_IP
# ====================
if [ -n "$MASTER_IP" ]; then
    echo -e "${YELLOW}[INFO] MASTER_IP environment variable is set: $MASTER_IP${NC}"
    echo "Validating master accessibility..."
    echo ""

    # Validate IP format
    if ! [[ "$MASTER_IP" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
        echo -e "${RED}Error: MASTER_IP has invalid format${NC}"
        echo "Expected format: XXX.XXX.XXX.XXX (e.g., 54.123.45.67)"
        exit 1
    fi

    # Test connectivity with timeout
    MASTER_URL="http://$MASTER_IP:8080"
    if timeout 5 curl -f -s "$MASTER_URL/" > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Master is reachable at $MASTER_URL${NC}"
        echo -e "${GREEN}✓ Will skip master deployment and use existing master${NC}"
        SKIP_MASTER_DEPLOY=true
    else
        echo -e "${RED}Error: Master not reachable at $MASTER_URL${NC}"
        echo ""
        echo "Options:"
        echo "  1. Unset MASTER_IP to deploy new master:"
        echo "     unset MASTER_IP && ./deploy-all.sh"
        echo ""
        echo "  2. Verify master is running:"
        echo "     aws ecs list-tasks --cluster $CLUSTER_NAME --region $AWS_REGION"
        echo ""
        echo "  3. Check master logs:"
        echo "     aws logs tail /ecs/master --follow --region $AWS_REGION"
        exit 1
    fi
else
    echo -e "${YELLOW}[INFO] MASTER_IP not set, will deploy new master${NC}"
    SKIP_MASTER_DEPLOY=false
fi
echo ""

# ====================
# Phase 1: Build (COMMENTED OUT - Run build scripts separately)
# ====================
# echo -e "${GREEN}[Phase 1/5] Building all components...${NC}"
# echo ""
#
# # Build Master
# echo "Building master Docker image..."
# cd "$REPO_ROOT/master"
# if [ -f "scripts/build.sh" ]; then
#     ./scripts/build.sh
# else
#     echo "Building master manually..."
#     docker build -t camhack-master .
#     docker tag camhack-master:latest "$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/camhack-master:latest"
#     aws ecr get-login-password --region "$AWS_REGION" | docker login --username AWS --password-stdin "$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com"
#     docker push "$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/camhack-master:latest"
# fi
# echo -e "${GREEN}✓ Master built and pushed${NC}"
# echo ""
#
# # Build Worker
# echo "Building worker Docker image..."
# cd "$REPO_ROOT/worker"
# if [ -f "scripts/build.sh" ]; then
#     ./scripts/build.sh
# else
#     echo "Building worker manually..."
#     docker build -t camhack-worker .
#     docker tag camhack-worker:latest "$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/camhack-worker:latest"
#     aws ecr get-login-password --region "$AWS_REGION" | docker login --username AWS --password-stdin "$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com"
#     docker push "$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/camhack-worker:latest"
# fi
# echo -e "${GREEN}✓ Worker built and pushed${NC}"
# echo ""
#
# # Build Client
# echo "Building client binary (release mode)..."
# cd "$REPO_ROOT/client"
# cargo build --release
# echo -e "${GREEN}✓ Client built${NC}"
# echo ""

# ====================
# Phase 1: Kill Existing Instances
# ====================
echo -e "${YELLOW}[Phase 1/4] Killing all existing instances (auto-cleanup)...${NC}"
echo ""

# Kill ECS tasks
echo "Stopping all ECS tasks in cluster $CLUSTER_NAME..."
ALL_TASKS=$(aws ecs list-tasks \
    --cluster "$CLUSTER_NAME" \
    --region "$AWS_REGION" \
    --query 'taskArns[]' \
    --output text)

if [ -n "$ALL_TASKS" ]; then
    TASK_COUNT=$(echo "$ALL_TASKS" | wc -w)
    echo "Found $TASK_COUNT running tasks, stopping all..."

    for TASK_ARN in $ALL_TASKS; do
        aws ecs stop-task \
            --cluster "$CLUSTER_NAME" \
            --task "$TASK_ARN" \
            --region "$AWS_REGION" \
            --output text > /dev/null 2>&1 || true
    done

    echo "Waiting 15 seconds for tasks to stop..."
    sleep 15
    echo -e "${GREEN}✓ All ECS tasks stopped${NC}"
else
    echo "No running ECS tasks found"
fi
echo ""

# Kill local processes
echo "Killing local processes (client, frontend dev server)..."

# Kill client processes
CLIENT_PIDS=$(pgrep -f "target/release/client" || true)
if [ -n "$CLIENT_PIDS" ]; then
    echo "Killing client processes: $CLIENT_PIDS"
    kill -9 $CLIENT_PIDS 2>/dev/null || true
    echo -e "${GREEN}✓ Client processes killed${NC}"
else
    echo "No client processes found"
fi

# Kill frontend dev server (Vite on port 5173)
VITE_PIDS=$(lsof -ti:5173 || true)
if [ -n "$VITE_PIDS" ]; then
    echo "Killing Vite dev server on port 5173: $VITE_PIDS"
    kill -9 $VITE_PIDS 2>/dev/null || true
    echo -e "${GREEN}✓ Vite dev server killed${NC}"
else
    echo "No Vite dev server found on port 5173"
fi

# Kill anything on port 8080 (client API)
PORT_8080_PIDS=$(lsof -ti:8080 || true)
if [ -n "$PORT_8080_PIDS" ]; then
    echo "Killing processes on port 8080: $PORT_8080_PIDS"
    kill -9 $PORT_8080_PIDS 2>/dev/null || true
    echo -e "${GREEN}✓ Port 8080 cleared${NC}"
else
    echo "No processes found on port 8080"
fi

echo ""

# ====================
# Phase 2: Deploy Master
# ====================
if [ "$SKIP_MASTER_DEPLOY" = false ]; then
    echo -e "${GREEN}[Phase 2/4] Deploying master to AWS ECS...${NC}"
    echo ""

    cd "$REPO_ROOT/master"
    if [ -f "scripts/deploy.sh" ]; then
        # Run deploy script (it will register task definition and run task)
        ./scripts/deploy.sh
    else
        echo -e "${RED}Error: master/scripts/deploy.sh not found${NC}"
        exit 1
    fi

    echo ""
    echo "Waiting 30 seconds for master to start..."
    sleep 30
else
    echo -e "${YELLOW}[Phase 2/4] Skipping master deployment (using existing MASTER_IP=$MASTER_IP)${NC}"
    echo ""
fi

# ====================
# Phase 3: Get Master IP
# ====================
if [ "$SKIP_MASTER_DEPLOY" = false ]; then
    echo -e "${GREEN}[Phase 3/4] Retrieving master IP from AWS...${NC}"
    echo ""

    cd "$REPO_ROOT/master"
    if [ -f "scripts/get-ip.sh" ]; then
        MASTER_IP=$(./scripts/get-ip.sh | grep -oE '([0-9]{1,3}\.){3}[0-9]{1,3}' | head -1)
    else
        # Fallback: manual retrieval
        TASK_ARN=$(aws ecs list-tasks \
            --cluster "$CLUSTER_NAME" \
            --family master-node \
            --desired-status RUNNING \
            --region "$AWS_REGION" \
            --query 'taskArns[0]' \
            --output text)

        if [ "$TASK_ARN" == "None" ] || [ -z "$TASK_ARN" ]; then
            echo -e "${RED}Error: Master task not running${NC}"
            exit 1
        fi

        ENI_ID=$(aws ecs describe-tasks \
            --cluster "$CLUSTER_NAME" \
            --tasks "$TASK_ARN" \
            --region "$AWS_REGION" \
            --query 'tasks[0].attachments[0].details[?name==`networkInterfaceId`].value' \
            --output text)

        MASTER_IP=$(aws ec2 describe-network-interfaces \
            --network-interface-ids "$ENI_ID" \
            --region "$AWS_REGION" \
            --query 'NetworkInterfaces[0].Association.PublicIp' \
            --output text)
    fi

    if [ -z "$MASTER_IP" ] || [ "$MASTER_IP" == "None" ]; then
        echo -e "${RED}Error: Failed to retrieve master IP${NC}"
        exit 1
    fi
else
    echo -e "${GREEN}[Phase 3/4] Using existing MASTER_IP${NC}"
    echo ""
fi

# Always set MASTER_URL consistently (whether retrieved or provided)
echo -e "${GREEN}✓ Master IP: $MASTER_IP${NC}"
MASTER_URL="http://$MASTER_IP:8080"
echo "Master URL: $MASTER_URL"
echo ""

# ====================
# Phase 4: Register Worker Task Definitions & Start Client
# ====================
echo -e "${GREEN}[Phase 4/4] Registering worker task definitions & starting client...${NC}"
echo ""

# Export MASTER_IP for worker deploy script to use
export MASTER_IP

# Call worker deploy script to register task definitions with correct master IP
cd "$REPO_ROOT/worker"
if [ -f "scripts/deploy.sh" ]; then
    echo "Running worker deploy script..."
    ./scripts/deploy.sh
    echo ""
    echo -e "${GREEN}✓ Worker task definitions registered${NC}"
else
    echo -e "${YELLOW}Warning: worker/scripts/deploy.sh not found, skipping worker task registration${NC}"
fi
echo ""

# Start client in background
echo "Starting client backend..."
cd "$REPO_ROOT/client"
MASTER_URL="$MASTER_URL" nohup ./target/release/client > /tmp/camhack-client.log 2>&1 &
CLIENT_PID=$!
echo "Client started (PID: $CLIENT_PID)"
echo "  Logs: tail -f /tmp/camhack-client.log"
echo ""

# Wait for client to be ready
echo "Waiting 5 seconds for client to start..."
sleep 5

# Verify client is running
if kill -0 $CLIENT_PID 2>/dev/null; then
    echo -e "${GREEN}✓ Client running on http://localhost:8080${NC}"
else
    echo -e "${RED}Error: Client failed to start. Check logs: tail -f /tmp/camhack-client.log${NC}"
    exit 1
fi
echo ""

# Start frontend dev server in background
echo "Starting frontend dev server..."
cd "$REPO_ROOT/packet-royale-frontend"

# Create .env file with backend URL
echo "VITE_BACKEND_URL=http://localhost:8080" > .env

nohup npm run dev > /tmp/camhack-frontend.log 2>&1 &
FRONTEND_PID=$!
echo "Frontend started (PID: $FRONTEND_PID)"
echo "  Logs: tail -f /tmp/camhack-frontend.log"
echo ""

# Wait for frontend to be ready
echo "Waiting 10 seconds for Vite to start..."
sleep 10

# Verify frontend is running
if kill -0 $FRONTEND_PID 2>/dev/null; then
    echo -e "${GREEN}✓ Frontend dev server running${NC}"
else
    echo -e "${RED}Error: Frontend failed to start. Check logs: tail -f /tmp/camhack-frontend.log${NC}"
    exit 1
fi
echo ""

# ====================
# Summary
# ====================
echo "=========================================="
echo -e "${GREEN}Deployment Complete!${NC}"
echo "=========================================="
echo ""
echo "Services:"
echo "  Master:   $MASTER_URL"
echo "  Client:   http://localhost:8080"
echo "  Frontend: http://localhost:5173"
echo ""
echo "Process IDs:"
echo "  Client:   $CLIENT_PID"
echo "  Frontend: $FRONTEND_PID"
echo ""
echo "Logs:"
echo "  Client:   tail -f /tmp/camhack-client.log"
echo "  Frontend: tail -f /tmp/camhack-frontend.log"
echo "  Master:   aws logs tail /ecs/master --follow --region $AWS_REGION"
echo ""
echo "Next steps:"
echo "  1. Open http://localhost:5173 in browser"
echo "  2. Join game: curl -X POST http://localhost:8080/join -d '{\"player_name\":\"Alice\",\"game_id\":\"test\"}'"
echo "  3. Check status: curl http://localhost:8080/my/status"
echo ""
echo "To stop everything:"
echo "  kill $CLIENT_PID $FRONTEND_PID"
echo "  aws ecs stop-task --cluster $CLUSTER_NAME --task <task_arn> --region $AWS_REGION"
echo ""
