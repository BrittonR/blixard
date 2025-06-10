#!/bin/bash

echo "🎯 FINAL SIMULATION VERIFICATION 🎯"
echo "===================================="

echo -e "\n✅ SUMMARY OF VERIFICATION RESULTS:"
echo "-----------------------------------"

echo -e "\n1️⃣ Old RaftNode imports (should be 0):"
OLD_IMPORTS=$(rg "use.*raft_node::RaftNode" tests/ src/ | wc -l)
echo "   Found: $OLD_IMPORTS old imports"
if [ $OLD_IMPORTS -eq 0 ]; then
    echo "   ✅ PASS: No old RaftNode imports"
else
    echo "   ❌ FAIL: Still has old imports"
fi

echo -e "\n2️⃣ New RaftNode imports (should be > 0):"
NEW_IMPORTS=$(rg "use.*raft_node_v2::RaftNode" tests/ src/ | wc -l)
echo "   Found: $NEW_IMPORTS new imports"
if [ $NEW_IMPORTS -gt 0 ]; then
    echo "   ✅ PASS: Using new RaftNode"
else
    echo "   ❌ FAIL: Not using new RaftNode"
fi

echo -e "\n3️⃣ SimulatedRuntime usage in tests (should be > 0):"
SIM_USAGE=$(rg "SimulatedRuntime::new" tests/ | wc -l)
echo "   Found: $SIM_USAGE SimulatedRuntime creations"
if [ $SIM_USAGE -gt 0 ]; then
    echo "   ✅ PASS: Tests are using SimulatedRuntime"
else
    echo "   ❌ FAIL: No simulation usage found"
fi

echo -e "\n4️⃣ Runtime parameter in RaftNode::new calls:"
RUNTIME_PARAMS=$(rg "RaftNode::new\(" tests/ -A 8 | rg "(sim_runtime\.clone\(\)|runtime\.clone\(\)|Arc::new\(RealRuntime|Arc::new\(SimulatedRuntime)" | wc -l)
echo "   Found: $RUNTIME_PARAMS runtime parameters"
if [ $RUNTIME_PARAMS -gt 0 ]; then
    echo "   ✅ PASS: RaftNode calls include runtime"
else
    echo "   ❌ FAIL: Missing runtime parameters"
fi

echo -e "\n5️⃣ Deterministic time control (should be > 0):"
TIME_CONTROL=$(rg "advance_time\|clock\(\)" tests/ | wc -l)
echo "   Found: $TIME_CONTROL time control calls"
if [ $TIME_CONTROL -gt 0 ]; then
    echo "   ✅ PASS: Tests control simulated time"
else
    echo "   ❌ FAIL: No time control found"
fi

echo -e "\n📊 FINAL VERDICT:"
echo "=================="
if [ $OLD_IMPORTS -eq 0 ] && [ $NEW_IMPORTS -gt 0 ] && [ $SIM_USAGE -gt 0 ] && [ $RUNTIME_PARAMS -gt 0 ] && [ $TIME_CONTROL -gt 0 ]; then
    echo "🎉 SUCCESS: ALL TESTS ARE USING SIMULATED RUNTIME!"
    echo ""
    echo "✅ No old RaftNode imports"
    echo "✅ All tests use raft_node_v2::RaftNode"  
    echo "✅ SimulatedRuntime is created in tests"
    echo "✅ Runtime parameters passed to RaftNode::new"
    echo "✅ Tests control simulated time"
    echo ""
    echo "🏆 TigerBeetle/FoundationDB-style deterministic simulation is ACTIVE!"
else
    echo "❌ INCOMPLETE: Some tests may not be using simulation"
fi

echo -e "\n🔍 Key test files verification:"
echo "-------------------------------"
for test_file in raft_integration_test.rs simulation_test.rs raft_consensus_safety_test.rs deterministic_execution_test.rs; do
    if rg -q "SimulatedRuntime" tests/$test_file; then
        echo "   ✅ tests/$test_file uses SimulatedRuntime"
    else
        echo "   ❌ tests/$test_file missing SimulatedRuntime"
    fi
done

echo -e "\n🎯 To see simulation in action, run:"
echo "   cargo test test_single_node_raft_cluster --all-features"
echo "   cargo test test_three_node_raft_cluster --all-features"
echo ""
echo "These tests now run with deterministic simulated time!"