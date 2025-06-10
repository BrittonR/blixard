#!/usr/bin/env bash
# Test bootstrapping a cluster

cd /home/brittonr/git/blixard/rust

# Clean up
pkill -f "blixard node" 2>/dev/null || true
rm -rf ./test-data
mkdir -p ./test-data/node1 ./test-data/node2 ./test-data/node3

echo "Starting Node 1 as bootstrap node with future peers..."
RUST_LOG=info,blixard::raft=debug,blixard::network=debug ./target/debug/blixard node \
    --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data/node1 \
    --peer 2:127.0.0.1:7002 --peer 3:127.0.0.1:7003 > node1.log 2>&1 &
NODE1_PID=$!

sleep 3

echo "Starting Node 2..."
RUST_LOG=info,blixard::raft=debug,blixard::network=debug ./target/debug/blixard node \
    --id 2 --bind 127.0.0.1:7002 --data-dir ./test-data/node2 \
    --peer 1:127.0.0.1:7001 --peer 3:127.0.0.1:7003 > node2.log 2>&1 &
NODE2_PID=$!

sleep 2

echo "Starting Node 3..."
RUST_LOG=info,blixard::raft=debug,blixard::network=debug ./target/debug/blixard node \
    --id 3 --bind 127.0.0.1:7003 --data-dir ./test-data/node3 \
    --peer 1:127.0.0.1:7001 --peer 2:127.0.0.1:7002 > node3.log 2>&1 &
NODE3_PID=$!

echo "Waiting for cluster to form..."
sleep 5

echo ""
echo "=== Checking for Leader Election ==="
echo "Node 1:"
grep -i "leader\|Leader\|state.*Leader" node1.log | tail -5 || echo "  No leader info yet"
echo ""
echo "Node 2:"
grep -i "leader\|Leader\|state.*Leader" node2.log | tail -5 || echo "  No leader info yet"
echo ""
echo "Node 3:"
grep -i "leader\|Leader\|state.*Leader" node3.log | tail -5 || echo "  No leader info yet"

echo ""
echo "=== Network Activity ==="
grep "Sending message\|Received message" node*.log | tail -10 || echo "No network messages yet"

echo ""
echo "=== Process Status ==="
ps aux | grep "[b]lixard node" | awk '{print $2, $11, $12, $13, $14, $15}'

# Kill nodes
kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null

echo ""
echo "Test complete. Full logs in node1.log, node2.log, node3.log"