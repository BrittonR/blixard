#!/usr/bin/env bash
# Simple test with just 2 nodes

set -e

echo "Building Blixard..."
cargo build

BINARY="./target/debug/blixard"

# Kill any existing nodes
pkill -f "blixard node" || true
sleep 1

# Clean data directories
rm -rf ./test-data
mkdir -p ./test-data/node1 ./test-data/node2

echo "Starting Node 1 on 127.0.0.1:7001..."
RUST_LOG=info $BINARY node --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data/node1 &
sleep 2

echo "Starting Node 2 on 127.0.0.1:7002 with Node 1 as peer..."
RUST_LOG=info $BINARY node --id 2 --bind 127.0.0.1:7002 --data-dir ./test-data/node2 --peer 1:127.0.0.1:7001 &

echo ""
echo "Two nodes should now be running and communicating."
echo "Check if they're establishing Raft consensus..."
echo ""

# Wait a bit and show some logs
sleep 5

echo "Recent logs:"
echo "=== NODE 1 ==="
tail -20 node1.log 2>/dev/null || echo "(No log file yet)"
echo ""
echo "=== NODE 2 ==="
tail -20 node2.log 2>/dev/null || echo "(No log file yet)"

echo ""
echo "Press Enter to stop..."
read

pkill -f "blixard node" || true