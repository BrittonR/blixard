#!/bin/bash
# Direct test of join_cluster RPC

set -e

echo "=== Direct Join Cluster Test ==="

# Clean up
rm -rf /tmp/blixard-test-*
killall blixard 2>/dev/null || true

# Start node 1
echo "Starting node 1..."
RUST_LOG=debug,blixard=trace,blixard_core=trace cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 > /tmp/node1_debug.log 2>&1 &
NODE1_PID=$!

# Wait for node 1
echo "Waiting for node 1..."
for i in {1..30}; do
    if curl -s http://127.0.0.1:8081/metrics > /dev/null 2>&1; then
        echo "Node 1 is up!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "Node 1 failed to start!"
        tail -50 /tmp/node1_debug.log
        exit 1
    fi
    sleep 1
done

# Show node 1 P2P info
echo "Node 1 bootstrap info:"
curl -s http://127.0.0.1:8081/bootstrap | jq .

# Start node 2 with trace logging
echo -e "\nStarting node 2 with trace logging..."
RUST_LOG=trace cargo run -- node --id 2 --bind 127.0.0.1:7002 --data-dir /tmp/blixard-test-2 --join-address http://127.0.0.1:8081 > /tmp/node2_trace.log 2>&1 &
NODE2_PID=$!

# Wait a bit for join attempt
echo "Waiting for join attempt..."
sleep 15

# Check if nodes are still running
if ps -p $NODE1_PID > /dev/null; then
    echo "Node 1 is still running"
else
    echo "Node 1 crashed!"
fi

if ps -p $NODE2_PID > /dev/null; then
    echo "Node 2 is still running"
else
    echo "Node 2 crashed!"
fi

# Show key log entries
echo -e "\n=== Key log entries from node 2 ==="
echo "Bootstrap fetch:"
grep -i "bootstrap" /tmp/node2_trace.log | tail -5 || echo "No bootstrap entries"

echo -e "\nP2P connection:"
grep -i "p2p.*connect\|connected to p2p peer" /tmp/node2_trace.log | tail -5 || echo "No P2P connection entries"

echo -e "\nJoin cluster RPC:"
grep -i "join.*cluster\|calling.*cluster" /tmp/node2_trace.log | tail -10 || echo "No join cluster entries"

echo -e "\nStream errors:"
grep -i "stream.*error\|connection lost\|finish" /tmp/node2_trace.log | tail -10 || echo "No stream errors"

echo -e "\nIroh transport traces:"
grep -i "iroh.*transport\|write_message\|read_message" /tmp/node2_trace.log | tail -20 || echo "No transport traces"

# Cleanup
echo -e "\n=== Cleanup ==="
kill $NODE1_PID $NODE2_PID 2>/dev/null || true

echo "Test complete!"