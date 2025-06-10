#!/bin/bash

echo "🔍 DETAILED RUNTIME VERIFICATION 🔍"
echo "===================================="

echo -e "\n1️⃣ Checking ALL RaftNode::new calls have 5 parameters (with runtime):"
echo "-----------------------------------------------------------------------"
echo "OLD format: RaftNode::new(id, addr, storage, peers) <- 4 params"
echo "NEW format: RaftNode::new(id, addr, storage, peers, runtime) <- 5 params"
echo ""

# Count parameters in each RaftNode::new call
rg "RaftNode::new\(" tests/ src/ -A 3 | grep -E "(RaftNode::new|\.await)" | while read line; do
    if [[ $line == *"RaftNode::new("* ]]; then
        echo "🔍 Found: $line"
        # Count commas to determine parameter count
        comma_count=$(echo "$line" | tr -cd ',' | wc -c)
        param_count=$((comma_count + 1))
        if [ $param_count -eq 5 ]; then
            echo "   ✅ Has 5 parameters (includes runtime)"
        else
            echo "   ❌ Has $param_count parameters (missing runtime?)"
        fi
    fi
done

echo -e "\n2️⃣ Verifying RaftNode struct types in tests:"
echo "----------------------------------------------"
rg "Vec<RaftNode" tests/ || echo "No untyped RaftNode vectors found (good!)"
rg "RaftNode<" tests/ | head -5

echo -e "\n3️⃣ Checking SimulatedRuntime is passed to RaftNode:"
echo "--------------------------------------------------"
rg "RaftNode::new\(" tests/ -A 5 | rg "runtime" | head -10

echo -e "\n4️⃣ Production code uses RealRuntime:"
echo "-----------------------------------"
rg "RealRuntime" src/

echo -e "\n5️⃣ Checking for any remaining old-style constructions:"
echo "------------------------------------------------------"
# This should find very few or none
rg "RaftNode::new\([^,]+,[^,]+,[^,]+,[^)]+\)\.await" tests/ src/ || echo "✅ No old-style 4-parameter calls found!"

echo -e "\n6️⃣ Verify specific test files are using simulation:"
echo "---------------------------------------------------"
echo "raft_integration_test.rs:"
rg "SimulatedRuntime" tests/raft_integration_test.rs | head -2

echo -e "\nsimulation_test.rs:"
rg "SimulatedRuntime" tests/simulation_test.rs | head -2

echo -e "\nraft_consensus_safety_test.rs:"
rg "SimulatedRuntime" tests/raft_consensus_safety_test.rs | head -2

echo -e "\n✅ VERIFICATION COMPLETE!"
echo "If you see ✅ checkmarks and SimulatedRuntime usage, all tests are using simulation!"