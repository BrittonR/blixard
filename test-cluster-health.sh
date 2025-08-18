#!/bin/bash

echo "=== BLIXARD CLUSTER HEALTH TEST ==="
echo ""

# Check running processes
echo "1. Running Processes:"
RUNNING=$(ps aux | grep "target/debug/blixard" | grep -v grep | wc -l)
echo "   Nodes running: $RUNNING/3"
ps aux | grep "target/debug/blixard" | grep -v grep | while read line; do
    echo "   - $line"
done
echo ""

# Check cluster membership
echo "2. Cluster Membership:"
VOTERS=$(grep "voters: {1, 2, 3}" node1.log | tail -1)
if [ -n "$VOTERS" ]; then
    echo "   ✅ All 3 nodes in cluster: $VOTERS"
else
    echo "   ❌ Cluster membership incomplete"
fi
echo ""

# Check leader election
echo "3. Leader Election:"
LEADER_MSGS_NODE2=$(grep -c "leader_id: Some(1)" node2.log 2>/dev/null || echo 0)
LEADER_MSGS_NODE3=$(grep -c "leader_id: Some(1)" node3.log 2>/dev/null || echo 0)
echo "   Node 1 (Leader): Active"
echo "   Node 2 leader recognitions: $LEADER_MSGS_NODE2"
echo "   Node 3 leader recognitions: $LEADER_MSGS_NODE3"
echo ""

# Check recent Raft activity
echo "4. Recent Raft Activity (last 10 seconds):"
RECENT_ACTIVITY=$(find . -name "node*.log" -exec grep "$(date '+%H:%M:' -d '10 seconds ago')" {} \; 2>/dev/null | wc -l)
echo "   Recent log entries: $RECENT_ACTIVITY"
echo ""

# Check message routing
echo "5. Raft Message Routing:"
RAFT_MESSAGES_NODE2=$(grep -c "RAFT_SERVICE: Successfully deserialized message" node2.log 2>/dev/null || echo 0)
RAFT_MESSAGES_NODE3=$(grep -c "RAFT_SERVICE: Successfully deserialized message" node3.log 2>/dev/null || echo 0)
echo "   Node 2 messages processed: $RAFT_MESSAGES_NODE2"
echo "   Node 3 messages processed: $RAFT_MESSAGES_NODE3"
echo ""

# Overall status
if [ $RUNNING -eq 3 ] && [ $LEADER_MSGS_NODE2 -gt 10 ] && [ $LEADER_MSGS_NODE3 -gt 10 ] && [ $RAFT_MESSAGES_NODE2 -gt 5 ] && [ $RAFT_MESSAGES_NODE3 -gt 5 ]; then
    echo "🎉 CLUSTER STATUS: HEALTHY"
else
    echo "⚠️  CLUSTER STATUS: ISSUES DETECTED"
fi