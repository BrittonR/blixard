#!/bin/bash
set -e

echo "🚀 Starting 3-node Blixard cluster with P2P transport"

# Clean up old data
echo "🧹 Cleaning up old data..."
rm -rf /tmp/blixard-cluster-test
mkdir -p /tmp/blixard-cluster-test

# Function to wait for port to be available
wait_for_port() {
    local port=$1
    local max_attempts=30
    local attempt=0
    
    while [ $attempt -lt $max_attempts ]; do
        # Use curl to check if port is open (works for HTTP endpoints)
        if curl -s -o /dev/null --connect-timeout 1 http://localhost:$port 2>/dev/null; then
            echo "✅ Port $port is ready"
            return 0
        fi
        # Alternative: check if anything is listening on the port using lsof
        if lsof -i :$port >/dev/null 2>&1; then
            echo "✅ Port $port is ready"
            return 0
        fi
        echo "⏳ Waiting for port $port to be ready... (attempt $((attempt + 1))/$max_attempts)"
        sleep 1
        attempt=$((attempt + 1))
    done
    
    echo "❌ Port $port did not become ready in time"
    return 1
}

# Start Node 1 (bootstrap node)
echo ""
echo "🔷 Starting Node 1 (bootstrap node)..."
RUST_LOG=blixard=debug,blixard_core=debug cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-cluster-test/node-1 > /tmp/blixard-cluster-test/node-1.log 2>&1 &
NODE1_PID=$!
echo "  PID: $NODE1_PID"

# Wait for node 1 to be ready
sleep 3
echo "⏳ Waiting for Node 1 to start..."
wait_for_port 8001 || { echo "❌ Node 1 failed to start"; kill $NODE1_PID 2>/dev/null; exit 1; }

# Verify bootstrap endpoint
echo "🔍 Verifying bootstrap endpoint..."
if curl -s http://127.0.0.1:8001/bootstrap | jq . > /dev/null 2>&1; then
    echo "✅ Bootstrap endpoint is working"
else
    echo "❌ Bootstrap endpoint is not working"
    kill $NODE1_PID 2>/dev/null
    exit 1
fi

# Start Node 2
echo ""
echo "🔷 Starting Node 2..."
RUST_LOG=blixard=debug,blixard_core=debug cargo run -- node --id 2 --bind 127.0.0.1:7002 --data-dir /tmp/blixard-cluster-test/node-2 --join-address http://127.0.0.1:8001 > /tmp/blixard-cluster-test/node-2.log 2>&1 &
NODE2_PID=$!
echo "  PID: $NODE2_PID"

# Wait for node 2 to be ready
sleep 3
echo "⏳ Waiting for Node 2 to start..."
wait_for_port 8002 || { echo "❌ Node 2 failed to start"; kill $NODE1_PID $NODE2_PID 2>/dev/null; exit 1; }

# Start Node 3
echo ""
echo "🔷 Starting Node 3..."
RUST_LOG=blixard=debug,blixard_core=debug cargo run -- node --id 3 --bind 127.0.0.1:7003 --data-dir /tmp/blixard-cluster-test/node-3 --join-address http://127.0.0.1:8001 > /tmp/blixard-cluster-test/node-3.log 2>&1 &
NODE3_PID=$!
echo "  PID: $NODE3_PID"

# Wait for node 3 to be ready
sleep 3
echo "⏳ Waiting for Node 3 to start..."
wait_for_port 8003 || { echo "❌ Node 3 failed to start"; kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null; exit 1; }

echo ""
echo "✅ All nodes started successfully!"
echo ""
echo "📊 Node PIDs:"
echo "  Node 1: $NODE1_PID"
echo "  Node 2: $NODE2_PID"
echo "  Node 3: $NODE3_PID"

# Save PIDs for later cleanup
echo "$NODE1_PID" > /tmp/blixard-cluster-test/node-1.pid
echo "$NODE2_PID" > /tmp/blixard-cluster-test/node-2.pid
echo "$NODE3_PID" > /tmp/blixard-cluster-test/node-3.pid

# Wait for leader election
echo ""
echo "⏳ Waiting for leader election..."
sleep 5

# Check cluster status
echo ""
echo "📊 Checking cluster status..."
if [ -f /tmp/blixard-cluster-test/node-1/node-1-registry.json ]; then
    export BLIXARD_NODE_ADDR=/tmp/blixard-cluster-test/node-1/node-1-registry.json
    cargo run -- cluster status --addr $BLIXARD_NODE_ADDR 2>/dev/null || echo "⚠️  Cluster status check failed"
else
    echo "⚠️  Node registry not found yet"
fi

echo ""
echo "📝 Log files:"
echo "  Node 1: /tmp/blixard-cluster-test/node-1.log"
echo "  Node 2: /tmp/blixard-cluster-test/node-2.log"
echo "  Node 3: /tmp/blixard-cluster-test/node-3.log"

echo ""
echo "🛑 To stop the cluster, run: ./scripts/stop_cluster.sh"
echo "📊 To check status: export BLIXARD_NODE_ADDR=/tmp/blixard-cluster-test/node-1/node-1-registry.json && cargo run -- cluster status --addr \$BLIXARD_NODE_ADDR"