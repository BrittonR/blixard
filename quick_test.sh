#!/usr/bin/env bash
# Quick test to see if nodes connect

cd /home/brittonr/git/blixard/rust

# Clean up
pkill -f "blixard node" 2>/dev/null || true
rm -rf ./test-data
mkdir -p ./test-data/node1 ./test-data/node2

echo "Starting Node 1..."
RUST_LOG=info,blixard::network=debug ./target/debug/blixard node \
    --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data/node1 > node1.log 2>&1 &
NODE1_PID=$!

sleep 2

echo "Starting Node 2 with Node 1 as peer..."
RUST_LOG=info,blixard::network=debug ./target/debug/blixard node \
    --id 2 --bind 127.0.0.1:7002 --data-dir ./test-data/node2 \
    --peer 1:127.0.0.1:7001 > node2.log 2>&1 &
NODE2_PID=$!

echo "Waiting for nodes to connect..."
sleep 3

echo ""
echo "=== Node 1 Recent Logs ==="
tail -30 node1.log | grep -E "(Starting|Adding peer|Received|Sending|leader|Leader)"

echo ""
echo "=== Node 2 Recent Logs ==="
tail -30 node2.log | grep -E "(Starting|Adding peer|Received|Sending|leader|Leader)"

# Show network activity
echo ""
echo "=== Network Connections ==="
netstat -an | grep 700[12] | grep ESTABLISHED || echo "No established connections yet"

# Kill nodes
kill $NODE1_PID $NODE2_PID 2>/dev/null

echo ""
echo "Test complete. Check node1.log and node2.log for full output."