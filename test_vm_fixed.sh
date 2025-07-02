#!/bin/bash
# Test VM creation with fixed Iroh node ID persistence

echo "Starting fresh test with node ID persistence..."

# Clean up
echo "Cleaning up old data..."
rm -rf /tmp/blixard-test-1
mkdir -p /tmp/blixard-test-1

# Start node with P2P enabled
echo -e "\n=== Starting node with P2P enabled ==="
export BLIXARD_P2P_ENABLED=true
RUST_LOG=info,blixard_core::transport=debug,blixard_core::p2p_manager=debug,blixard_core::iroh_transport_v2=debug \
cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1 &
NODE_PID=$!

echo "Waiting for node to initialize and write registry..."
sleep 20

# Check if registry file was created
echo -e "\n=== Checking registry file ==="
if [ -f "/tmp/blixard-test-1/node-1-registry.json" ]; then
    echo "Registry file created:"
    cat /tmp/blixard-test-1/node-1-registry.json
else
    echo "ERROR: Registry file not created!"
fi

# Check if secret key was persisted
echo -e "\n=== Checking secret key persistence ==="
if [ -f "/tmp/blixard-test-1/iroh/secret_key" ]; then
    echo "Secret key persisted successfully"
    echo "Key size: $(wc -c < /tmp/blixard-test-1/iroh/secret_key) bytes"
else
    echo "ERROR: Secret key not persisted!"
fi

echo -e "\n=== Testing VM Creation ==="
# Pass the registry file path directly instead of just the address
RUST_LOG=debug,blixard_core::transport::iroh_client=trace,blixard::client=debug \
cargo run --bin blixard -- vm create --name test-vm --vcpus 2 --memory 1024 --addr /tmp/blixard-test-1/node-1-registry.json

echo -e "\n=== Testing VM List ==="
cargo run --bin blixard -- vm list --addr /tmp/blixard-test-1/node-1-registry.json

echo -e "\n=== Cleanup ==="
kill $NODE_PID 2>/dev/null
wait $NODE_PID 2>/dev/null

echo -e "\nDone!"