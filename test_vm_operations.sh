#!/bin/bash
# Test VM operations through Iroh P2P

echo "Building blixard..."
cargo build --bin blixard

echo -e "\nStarting node 1 (leader)..."
RUST_LOG=info cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 &
NODE1_PID=$!

# Wait for node to start and initialize
sleep 10

echo -e "\nCreating VM..."
cargo run --bin blixard -- vm create --name test-vm --vcpus 2 --memory 1024

echo -e "\nListing VMs..."
cargo run --bin blixard -- vm list

echo -e "\nGetting VM status..."
cargo run --bin blixard -- vm status --name test-vm

echo -e "\nCleaning up..."
kill $NODE1_PID 2>/dev/null
rm -rf /tmp/blixard-test-1

echo "Test complete!"