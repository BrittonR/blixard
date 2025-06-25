#!/bin/bash
# Test gRPC endpoints using the CLI

echo "=== Creating a VM via CLI to trigger gRPC metrics ==="
cargo run -- vm create --name test-vm-1 --vcpus 2 --memory 1024

echo
echo "=== Checking metrics after VM creation ==="
echo "VM metrics:"
curl -s http://127.0.0.1:8005/metrics | grep -E "vm_" | grep -v "^#" | head -10

echo
echo "gRPC metrics:"
curl -s http://127.0.0.1:8005/metrics | grep -E "grpc_" | grep -v "^#" | head -10