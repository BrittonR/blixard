#!/bin/bash

echo "🛑 Stopping Blixard cluster..."

# Read PIDs if available
if [ -f /tmp/blixard-cluster-test/node-1.pid ]; then
    NODE1_PID=$(cat /tmp/blixard-cluster-test/node-1.pid)
    if kill -0 $NODE1_PID 2>/dev/null; then
        echo "  Stopping Node 1 (PID: $NODE1_PID)..."
        kill $NODE1_PID
    fi
fi

if [ -f /tmp/blixard-cluster-test/node-2.pid ]; then
    NODE2_PID=$(cat /tmp/blixard-cluster-test/node-2.pid)
    if kill -0 $NODE2_PID 2>/dev/null; then
        echo "  Stopping Node 2 (PID: $NODE2_PID)..."
        kill $NODE2_PID
    fi
fi

if [ -f /tmp/blixard-cluster-test/node-3.pid ]; then
    NODE3_PID=$(cat /tmp/blixard-cluster-test/node-3.pid)
    if kill -0 $NODE3_PID 2>/dev/null; then
        echo "  Stopping Node 3 (PID: $NODE3_PID)..."
        kill $NODE3_PID
    fi
fi

# Also try to kill any remaining blixard processes
pkill -f "blixard node" 2>/dev/null || true

echo "✅ Cluster stopped"