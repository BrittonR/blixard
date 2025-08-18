#!/bin/bash
# Simple script to run and monitor the cluster

echo "🚀 Starting three-node Blixard cluster..."
echo ""

# Clean up
pkill -9 -f blixard 2>/dev/null
rm -rf test_node* node*.log

# Start all nodes
./test-three-node-fixed.sh

echo ""
echo "⏳ Waiting for cluster to stabilize (15 seconds)..."
sleep 15

echo ""
echo "📊 Cluster Status:"
echo "=================="
ps aux | grep blixard | grep -v grep | wc -l | xargs -I {} echo "✅ Nodes running: {}/3"

echo ""
echo "🔄 Live Heartbeat Activity (last 5 seconds):"
for i in 1 2 3; do 
    echo -n "Node $i: "
    tail -100 node$i.log | grep -c MsgHeartbeat | xargs -I {} echo "{} heartbeats"
done

echo ""
echo "📝 Commands:"
echo "  Watch logs:    tail -f node*.log"
echo "  Stop cluster:  pkill -f blixard"
echo "  Check status:  ps aux | grep blixard"

