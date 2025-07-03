#!/bin/bash
# Simple P2P cluster formation test

set -e

echo "=== Testing P2P cluster formation ==="

# Clean up any existing test data
rm -rf /tmp/blixard-test-*

# Build the project
echo "Building blixard..."
cd /home/brittonr/git/blixard
cargo build

# Start node 1 (bootstrap node)
echo "Starting node 1 (bootstrap)..."
RUST_LOG=info,blixard=debug cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 &
NODE1_PID=$!
sleep 5

# Get bootstrap info from node 1
echo "Getting bootstrap info from node 1..."
BOOTSTRAP_INFO=$(curl -s http://127.0.0.1:8081/bootstrap)
echo "Bootstrap info: $BOOTSTRAP_INFO"

# Start node 2 and join using P2P bootstrap
echo "Starting node 2..."
RUST_LOG=info,blixard=debug cargo run -- node --id 2 --bind 127.0.0.1:7002 --data-dir /tmp/blixard-test-2 --join-address http://127.0.0.1:8081 &
NODE2_PID=$!
sleep 5

# Check cluster status on node 1
echo "Checking cluster status on node 1..."
RESPONSE=$(cargo run -- cluster status --server 127.0.0.1:7001 2>&1 || true)
echo "Node 1 cluster status:"
echo "$RESPONSE"

# Check cluster status on node 2
echo "Checking cluster status on node 2..."
RESPONSE=$(cargo run -- cluster status --server 127.0.0.1:7002 2>&1 || true)
echo "Node 2 cluster status:"
echo "$RESPONSE"

# Try to create a VM on the cluster
echo "Testing VM creation..."
RESPONSE=$(cargo run -- vm create --name test-vm --server 127.0.0.1:7001 2>&1 || true)
echo "VM create response:"
echo "$RESPONSE"

# List VMs
echo "Listing VMs..."
RESPONSE=$(cargo run -- vm list --server 127.0.0.1:7001 2>&1 || true)
echo "VM list response:"
echo "$RESPONSE"

# Cleanup
echo "Cleaning up..."
kill $NODE1_PID $NODE2_PID 2>/dev/null || true
wait $NODE1_PID $NODE2_PID 2>/dev/null || true

echo "=== Test complete ==="