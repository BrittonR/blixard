#!/bin/bash
# Script to migrate simulation tests from blixard_core imports to local imports

echo "Migrating simulation tests to use local types..."

# List of test files to migrate
TEST_FILES=(
    "tests/three_node_cluster_tests.rs"
    "tests/three_node_manual_test.rs"
    "tests/raft_quick_test.rs"
    "tests/snapshot_tests.rs"
    "tests/storage_edge_case_tests.rs"
    "tests/storage_performance_benchmarks.rs"
    "tests/test_isolation_verification.rs"
    "tests/node_lifecycle_integration_tests.rs"
    "tests/peer_management_tests.rs"
    "tests/port_allocation_stress_test.rs"
    "tests/grpc_service_tests.rs"
    "tests/join_cluster_config_test.rs"
    "tests/network_partition_storage_tests.rs"
    "tests/distributed_storage_consistency_tests.rs"
    "tests/cluster_integration_tests.rs"
    "tests/cli_cluster_commands_test.rs"
    "tests/common/mod.rs"
    "tests/common/raft_test_utils.rs"
)

for file in "${TEST_FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "Processing $file..."
        
        # Replace blixard_core imports with blixard_simulation imports
        sed -i 's/use blixard_core::{/use blixard_simulation::{/g' "$file"
        sed -i 's/use blixard_core::/use blixard_simulation::/g' "$file"
        sed -i 's/blixard_core::/blixard_simulation::/g' "$file"
        
        # Fix test_helpers imports
        sed -i 's/test_helpers::{TestNode, TestCluster, timing}/TestNode, TestCluster, timing/g' "$file"
        sed -i 's/test_helpers::{TestNode, TestCluster}/TestNode, TestCluster/g' "$file"
        sed -i 's/test_helpers::{TestNode, timing}/TestNode, timing/g' "$file"
        sed -i 's/test_helpers::{TestCluster, timing}/TestCluster, timing/g' "$file"
        sed -i 's/test_helpers::{PortAllocator, TestNode}/PortAllocator, TestNode/g' "$file"
        sed -i 's/test_helpers::PortAllocator/PortAllocator/g' "$file"
        sed -i 's/test_helpers::timing/timing/g' "$file"
        sed -i 's/test_helpers::/blixard_simulation::/g' "$file"
        
        # Fix error imports
        sed -i 's/error::{BlixardError, BlixardResult}/BlixardError, BlixardResult/g' "$file"
        sed -i 's/error::BlixardError/BlixardError/g' "$file"
        sed -i 's/error::BlixardResult/BlixardResult/g' "$file"
        
        # Fix node_shared imports
        sed -i 's/node_shared::{SharedNodeState, PeerInfo}/SharedNodeState, PeerInfo/g' "$file"
        sed -i 's/node_shared::SharedNodeState/SharedNodeState/g' "$file"
        sed -i 's/node_shared::PeerInfo/PeerInfo/g' "$file"
        sed -i 's/node_shared::RaftStatus/RaftStatus/g' "$file"
        
        # Fix raft_manager imports
        sed -i 's/raft_manager::{TaskSpec, ResourceRequirements}/TaskSpec, ResourceRequirements/g' "$file"
        sed -i 's/raft_manager::TaskSpec/TaskSpec/g' "$file"
        sed -i 's/raft_manager::ResourceRequirements/ResourceRequirements/g' "$file"
        
        # Fix types imports
        sed -i 's/types::NodeConfig/NodeConfig/g' "$file"
        
        # Fix node imports
        sed -i 's/node::Node/Node/g' "$file"
        
        # Remove proto:: prefix since we re-export everything
        sed -i 's/proto::{/blixard_simulation::{/g' "$file"
        sed -i 's/proto::/blixard_simulation::/g' "$file"
        
        # Add use blixard_simulation::*; if not present and file uses these types
        if ! grep -q "use blixard_simulation::\*;" "$file" && grep -q "blixard_simulation::" "$file"; then
            # Add after the first use statement or at the beginning
            if grep -q "^use " "$file"; then
                sed -i '0,/^use /{s/^use /use blixard_simulation::*;\nuse /}' "$file"
            else
                sed -i '1i use blixard_simulation::*;' "$file"
            fi
        fi
    fi
done

echo "Migration complete! Now you need to:"
echo "1. Review the changes to ensure correctness"
echo "2. Add any missing type implementations to simulation/src/types.rs"
echo "3. Update test logic to work with simplified test helpers"
echo "4. Run 'cargo check' in the simulation directory to find remaining issues"