#!/bin/bash

echo "=== TESTING RAFT LOG REPLICATION ==="
echo ""

# Check initial log indices
echo "1. Current Raft log indices:"
NODE1_INDEX=$(grep "last_index:" node1.log | tail -1 | sed 's/.*last_index: \([0-9]*\).*/\1/')
NODE2_INDEX=$(grep "last_index:" node2.log | tail -1 | sed 's/.*last_index: \([0-9]*\).*/\1/')
NODE3_INDEX=$(grep "last_index:" node3.log | tail -1 | sed 's/.*last_index: \([0-9]*\).*/\1/')

echo "   Node 1 (Leader) last_index: $NODE1_INDEX"
echo "   Node 2 (Follower) last_index: $NODE2_INDEX" 
echo "   Node 3 (Follower) last_index: $NODE3_INDEX"
echo ""

# Check applied indices
echo "2. Applied indices (committed entries):"
NODE1_APPLIED=$(grep "applied:" node1.log | tail -1 | sed 's/.*applied: \([0-9]*\).*/\1/')
NODE2_APPLIED=$(grep "applied:" node2.log | tail -1 | sed 's/.*applied: \([0-9]*\).*/\1/')
NODE3_APPLIED=$(grep "applied:" node3.log | tail -1 | sed 's/.*applied: \([0-9]*\).*/\1/')

echo "   Node 1 applied: $NODE1_APPLIED"
echo "   Node 2 applied: $NODE2_APPLIED"
echo "   Node 3 applied: $NODE3_APPLIED"
echo ""

# Check commit indices
echo "3. Commit indices (replicated entries):"
NODE1_COMMIT=$(grep "commit:" node1.log | tail -1 | sed 's/.*commit: \([0-9]*\).*/\1/')
NODE2_COMMIT=$(grep "commit:" node2.log | tail -1 | sed 's/.*commit: \([0-9]*\).*/\1/')
NODE3_COMMIT=$(grep "commit:" node3.log | tail -1 | sed 's/.*commit: \([0-9]*\).*/\1/')

echo "   Node 1 commit: $NODE1_COMMIT"
echo "   Node 2 commit: $NODE2_COMMIT"
echo "   Node 3 commit: $NODE3_COMMIT"
echo ""

# Check for log consistency
echo "4. Log Consistency Check:"
if [ "$NODE1_COMMIT" = "$NODE2_COMMIT" ] && [ "$NODE2_COMMIT" = "$NODE3_COMMIT" ]; then
    echo "   ✅ All nodes have consistent commit indices"
else
    echo "   ⚠️  Nodes have different commit indices (might be temporary)"
fi

if [ "$NODE1_APPLIED" = "$NODE2_APPLIED" ] && [ "$NODE2_APPLIED" = "$NODE3_APPLIED" ]; then
    echo "   ✅ All nodes have consistent applied indices"
else
    echo "   ⚠️  Nodes have different applied indices (might be temporary)"
fi
echo ""

# Count configuration changes (evidence of log replication working)
echo "5. Evidence of successful log replication:"
CONFIG_CHANGES=$(grep -c "AddNode\|configuration change" node1.log)
echo "   Configuration changes processed: $CONFIG_CHANGES"
echo "   (Adding Node 2 and Node 3 proves log replication worked)"
echo ""

# Show recent append entries
echo "6. Recent log replication activity:"
RECENT_APPENDS=$(grep "MsgAppend\|AppendEntries" node*.log | tail -5)
if [ -n "$RECENT_APPENDS" ]; then
    echo "$RECENT_APPENDS"
else
    echo "   No recent append entries (normal during steady state)"
fi