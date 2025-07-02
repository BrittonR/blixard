#!/bin/bash
# Test VM operations with node in foreground

echo "Starting node in foreground..."
echo "Once node is running, open another terminal and run:"
echo "  cargo run --bin blixard -- vm create --name test-vm --vcpus 2 --memory 1024"
echo ""

rm -rf /tmp/blixard-test-1

RUST_LOG=info,blixard=debug,blixard_core=debug cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir /tmp/blixard-test-1