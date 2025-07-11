#!/bin/bash
# Script to fix all warnings in Blixard

echo "Starting warning cleanup..."

# Fix unused imports
echo "Fixing unused imports..."

# resource_monitor.rs - unused import: crate::metrics_otel
sed -i '/use crate::metrics_otel/d' blixard-core/src/resource_monitor.rs

# vm_scheduler_modules/mod.rs - unused import: crate::metrics_otel  
sed -i '/use crate::metrics_otel/d' blixard-core/src/vm_scheduler_modules/mod.rs

# storage/database_transaction.rs - unused import: ReadableMultimapTable
sed -i '/ReadableMultimapTable/d' blixard-core/src/storage/database_transaction.rs

# vm_scheduler_modules/resource_analysis.rs - unused import: redb::ReadableTable
sed -i '/use redb::ReadableTable/d' blixard-core/src/vm_scheduler_modules/resource_analysis.rs

# iroh_transport_v2.rs - unused import: tokio::io::AsyncWriteExt
sed -i '/use tokio::io::AsyncWriteExt/d' blixard-core/src/iroh_transport_v2.rs

# vm_health_state_manager.rs - unused import: abstractions::time::Clock
sed -i '/use abstractions::time::Clock/d' blixard-core/src/vm_health_state_manager.rs

echo "Imports fixed."

# Fix unused variables by prefixing with underscore
echo "Fixing unused variables..."

# vopr/test_harness.rs - multiple unused variables
sed -i 's/client_id/_client_id/g' blixard-core/src/vopr/test_harness.rs
sed -i 's/request_id/_request_id/g' blixard-core/src/vopr/test_harness.rs
sed -i 's/|proposal|/|_proposal|/g' blixard-core/src/vopr/test_harness.rs
sed -i 's/(vm_id)/(_vm_id)/g' blixard-core/src/vopr/test_harness.rs
sed -i 's/(task_id, command))/(_task_id, _command))/g' blixard-core/src/vopr/test_harness.rs

# metrics_otel.rs - unused variables
sed -i 's/let vm_attr =/let _vm_attr =/g' blixard-core/src/metrics_otel.rs
sed -i 's/let confidence_attr =/let _confidence_attr =/g' blixard-core/src/metrics_otel.rs
sed -i 's/|vm_name|/|_vm_name|/g' blixard-core/src/metrics_otel.rs
sed -i 's/|image_id|/|_image_id|/g' blixard-core/src/metrics_otel.rs

# linearizability.rs - unused variable
sed -i 's/|(entries)|/|(_entries)|/g' blixard-core/src/linearizability.rs

echo "Variables fixed."

# Fix unnecessary mutability
echo "Fixing unnecessary mutability..."

# config/raft.rs - variable does not need to be mutable
sed -i 's/let mut restart_count = 0/let restart_count = 0/g' blixard-core/src/config/raft.rs

echo "Mutability fixed."

# Fix value assigned but never read
echo "Fixing assigned but never read..."

# node.rs - restart_count assigned but never read
# This needs manual inspection - commenting out the increment for now
sed -i 's/restart_count += 1;/\/\/ restart_count += 1; \/\/ TODO: Use or remove/g' blixard-core/src/node.rs

echo "Assignments fixed."

echo "Warning cleanup complete!"