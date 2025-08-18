#!/bin/bash

# Source cleanup functions for consistency
source ./test-cleanup.sh 2>/dev/null || true

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
VOTERS=$(strings node1.log 2>/dev/null | grep "voters: {1, 2, 3}" | tail -1)
if [ -n "$VOTERS" ]; then
    echo "   ✅ All 3 nodes in cluster: $VOTERS"
else
    echo "   ❌ Cluster membership incomplete"
fi
echo ""

# Check leader election
echo "3. Leader Election:"
LEADER_MSGS_NODE2=$(strings node2.log 2>/dev/null | grep -c "leader_id: Some(1)" || echo "0")
LEADER_MSGS_NODE3=$(strings node3.log 2>/dev/null | grep -c "leader_id: Some(1)" || echo "0")
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
RAFT_MESSAGES_NODE2=$(strings node2.log 2>/dev/null | grep -c "RAFT_SERVICE: Successfully deserialized message" || echo "0")
RAFT_MESSAGES_NODE3=$(strings node3.log 2>/dev/null | grep -c "RAFT_SERVICE: Successfully deserialized message" || echo "0")
echo "   Node 2 messages processed: $RAFT_MESSAGES_NODE2"
echo "   Node 3 messages processed: $RAFT_MESSAGES_NODE3"
echo ""

# Overall status
# Ensure variables have default values and are numeric
RUNNING=${RUNNING:-0}
LEADER_MSGS_NODE2=${LEADER_MSGS_NODE2:-0}
LEADER_MSGS_NODE3=${LEADER_MSGS_NODE3:-0}
RAFT_MESSAGES_NODE2=${RAFT_MESSAGES_NODE2:-0}
RAFT_MESSAGES_NODE3=${RAFT_MESSAGES_NODE3:-0}

# Remove any non-numeric characters (like newlines) from variables
RUNNING=$(echo "$RUNNING" | tr -d '\n' | grep -o '[0-9]*' | head -1)
LEADER_MSGS_NODE2=$(echo "$LEADER_MSGS_NODE2" | tr -d '\n' | grep -o '[0-9]*' | head -1)
LEADER_MSGS_NODE3=$(echo "$LEADER_MSGS_NODE3" | tr -d '\n' | grep -o '[0-9]*' | head -1)
RAFT_MESSAGES_NODE2=$(echo "$RAFT_MESSAGES_NODE2" | tr -d '\n' | grep -o '[0-9]*' | head -1)
RAFT_MESSAGES_NODE3=$(echo "$RAFT_MESSAGES_NODE3" | tr -d '\n' | grep -o '[0-9]*' | head -1)

# Ensure empty values become 0
RUNNING=${RUNNING:-0}
LEADER_MSGS_NODE2=${LEADER_MSGS_NODE2:-0}
LEADER_MSGS_NODE3=${LEADER_MSGS_NODE3:-0}
RAFT_MESSAGES_NODE2=${RAFT_MESSAGES_NODE2:-0}
RAFT_MESSAGES_NODE3=${RAFT_MESSAGES_NODE3:-0}

if [ "$RUNNING" -eq 3 ] && [ "$LEADER_MSGS_NODE2" -gt 10 ] && [ "$LEADER_MSGS_NODE3" -gt 10 ] && [ "$RAFT_MESSAGES_NODE2" -gt 5 ] && [ "$RAFT_MESSAGES_NODE3" -gt 5 ]; then
    echo "🎉 CLUSTER STATUS: HEALTHY"
else
    echo "⚠️  CLUSTER STATUS: ISSUES DETECTED"
fi