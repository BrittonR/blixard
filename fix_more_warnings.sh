#!/bin/bash
# Script to fix remaining warnings in Blixard

echo "Fixing unused variable warnings..."

# Fix unused variables in transport/iroh_peer_connector.rs
sed -i 's/let node = /let _node = /g' blixard-core/src/transport/iroh_peer_connector.rs
sed -i 's/let connections = /let _connections = /g' blixard-core/src/transport/iroh_peer_connector.rs

# Fix unused variables in transport/services/vm.rs
sed -i 's/let strategy = /let _strategy = /g' blixard-core/src/transport/services/vm.rs

# Fix unused variables in transport/services/vm_image.rs
sed -i 's/let metadata = /let _metadata = /g' blixard-core/src/transport/services/vm_image.rs
sed -i 's/let size = /let _size = /g' blixard-core/src/transport/services/vm_image.rs
sed -i 's/let image_hash = /let _image_hash = /g' blixard-core/src/transport/services/vm_image.rs
sed -i 's/let request_id = /let _request_id = /g' blixard-core/src/transport/services/vm_image.rs
sed -i 's/let p2p_manager = /let _p2p_manager = /g' blixard-core/src/transport/services/vm_image.rs

# Fix unused config variables in vm_health_* files
sed -i 's/Some(config)/Some(_config)/g' blixard-core/src/vm_health_monitor.rs
sed -i 's/Some(config)/Some(_config)/g' blixard-core/src/vm_health_recovery_coordinator.rs
sed -i 's/Some(config)/Some(_config)/g' blixard-core/src/vm_health_scheduler.rs

# Fix unused variables in vopr files
sed -i 's/let operation = /let _operation = /g' blixard-core/src/vopr/*.rs
sed -i 's/Err(error)/Err(_error)/g' blixard-core/src/vopr/*.rs
sed -i 's/let rng = /let _rng = /g' blixard-core/src/vopr/*.rs

# Fix other unused variables
sed -i 's/|node|/|_node|/g' blixard-core/src/transport/iroh_peer_connector.rs
sed -i 's/(old_acceleration)/(old_acceleration: _)/g' blixard-core/src/*.rs
sed -i 's/(factor)/(factor: _)/g' blixard-core/src/*.rs
sed -i 's/, invariant/, _invariant/g' blixard-core/src/*.rs

echo "Warnings fixed!"