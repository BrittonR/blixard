#!/bin/bash
# Test gRPC endpoints to generate metrics

NODE_ID=7005
GRPC_PORT=7005
METRICS_PORT=8005

echo "=== Testing gRPC endpoints to generate metrics ==="
echo

# First, let's check if the node is running
if ! nc -z localhost $GRPC_PORT 2>/dev/null; then
    echo "Error: Node not running on port $GRPC_PORT"
    echo "Starting node..."
    cargo run -- node --id 1 --bind 127.0.0.1:$GRPC_PORT --data-dir /tmp/blixard-test-$NODE_ID &
    NODE_PID=$!
    echo "Started node with PID $NODE_PID"
    sleep 5
else
    echo "Node already running on port $GRPC_PORT"
fi

echo
echo "=== Health Check ==="
# Health check doesn't exist in CLI, but we can use cluster status
cargo run -- cluster status --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Creating VMs ==="
cargo run -- vm create --name test-vm-1 --vcpus 2 --memory 1024 --node 127.0.0.1:$GRPC_PORT
cargo run -- vm create --name test-vm-2 --vcpus 4 --memory 2048 --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Starting a VM ==="
cargo run -- vm start --name test-vm-1 --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Listing VMs ==="
cargo run -- vm list --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Getting VM Status ==="
cargo run -- vm status --name test-vm-1 --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Stopping a VM ==="
cargo run -- vm stop --name test-vm-1 --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Deleting a VM ==="
cargo run -- vm delete --name test-vm-2 --node 127.0.0.1:$GRPC_PORT

echo
echo "=== Checking metrics after gRPC operations ==="
echo
echo "gRPC request metrics:"
curl -s http://127.0.0.1:$METRICS_PORT/metrics | grep -E "grpc_requests_total|grpc_request_duration|grpc_requests_failed" | grep -v "^#" | sort | head -20

echo
echo "VM operation metrics:"
curl -s http://127.0.0.1:$METRICS_PORT/metrics | grep -E "vm_total|vm_running|vm_create_total|vm_create_duration|vm_create_failed" | grep -v "^#" | sort | head -20

echo
echo "Storage operation metrics:"
curl -s http://127.0.0.1:$METRICS_PORT/metrics | grep -E "storage_reads_total|storage_writes_total|storage_write_duration|storage_read_duration" | grep -v "^#" | sort | head -20

# Clean up if we started the node
if [ ! -z "$NODE_PID" ]; then
    echo
    echo "Cleaning up node process $NODE_PID..."
    kill $NODE_PID 2>/dev/null
fi