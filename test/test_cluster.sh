#!/bin/bash
# test_proper_cluster.sh

set -e

echo "=== Proper Blixard Cluster Testing ==="

# 1. Start actual cluster nodes (these run Khepri)
echo "Starting cluster nodes..."
gleam run -m service_manager -- --join-cluster &
NODE1_PID=$!
sleep 5

gleam run -m service_manager -- --join-cluster &
NODE2_PID=$!
sleep 5

# 2. Now use CLI commands (these should NOT start Khepri)
echo "Testing service operations..."
gleam run -m service_manager -- start --user test-http-server
sleep 2

gleam run -m service_manager -- list
gleam run -m service_manager -- status --user test-http-server

# 3. Verify from another CLI call
echo "Verifying from new CLI instance..."
gleam run -m service_manager -- list

# Cleanup
kill $NODE1_PID $NODE2_PID 2>/dev/null || true
