#!/bin/bash

echo "🔍 VERIFYING SIMULATION TESTING IMPLEMENTATION 🔍"
echo "================================================"

echo -e "\n1️⃣ Checking for old RaftNode usage (should be NONE):"
echo "-----------------------------------------------------"
rg "use.*raft_node::RaftNode" tests/ src/ || echo "✅ No old RaftNode imports found!"

echo -e "\n2️⃣ Checking for new RaftNode usage (should find many):"
echo "-------------------------------------------------------"
rg "use.*raft_node_v2::RaftNode" tests/ src/

echo -e "\n3️⃣ Verifying SimulatedRuntime usage in tests:"
echo "----------------------------------------------"
rg "SimulatedRuntime::new" tests/

echo -e "\n4️⃣ Checking RaftNode construction with runtime parameter:"
echo "---------------------------------------------------------"
rg "RaftNode::new\(.*SimulatedRuntime" tests/ -A 1

echo -e "\n5️⃣ Checking for any RaftNode::new without 5 parameters:"
echo "--------------------------------------------------------"
# Old RaftNode had 4 params, new one has 5 (added runtime)
rg "RaftNode::new\([^,]+,[^,]+,[^,]+,[^,]+\)" tests/ src/ || echo "✅ No 4-parameter RaftNode::new calls found!"

echo -e "\n6️⃣ Verifying deterministic time control in tests:"
echo "-------------------------------------------------"
rg "runtime\.(advance_time|clock\(\))" tests/

echo -e "\n7️⃣ Checking for Instant::now() usage (should be minimal):"
echo "----------------------------------------------------------"
rg "Instant::now\(\)" src/runtime/simulation.rs tests/ || echo "✅ No Instant::now() in simulation code!"

echo -e "\n8️⃣ Feature flag check - simulation tests:"
echo "-----------------------------------------"
rg "#\[cfg\(feature = \"simulation\"\)\]" tests/ | head -5

echo -e "\n✅ VERIFICATION COMPLETE!"