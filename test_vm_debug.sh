#!/bin/bash
# Debug VM creation

echo "Cleaning up..."
rm -rf /tmp/blixard-test-1
mkdir -p /tmp/blixard-test-1

echo "Starting node with debug logging and P2P enabled..."
# Enable P2P explicitly since it's disabled by default
export BLIXARD_P2P_ENABLED=true
RUST_LOG=info,blixard_core::transport::iroh_client=debug,blixard_core::transport::iroh_vm_service=debug,blixard::client=debug,blixard_core::node=debug \
cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 &
NODE_PID=$!

echo "Waiting for node to initialize..."
sleep 15

echo -e "\n=== Testing VM Creation ==="
RUST_LOG=debug,blixard_core::transport::iroh_client=trace,blixard::client=debug \
cargo run --bin blixard -- vm create --name test-vm --vcpus 2 --memory 1024

echo -e "\n=== Cleanup ==="
kill $NODE_PID 2>/dev/null
wait $NODE_PID 2>/dev/null

echo "Done!"