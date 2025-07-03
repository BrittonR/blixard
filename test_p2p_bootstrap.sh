#!/bin/bash
# Test P2P bootstrap mechanism for multi-node cluster

echo "=== Testing P2P Bootstrap Mechanism ==="
echo ""

# Clean up first
echo "🧹 Cleaning up old data..."
./scripts/cleanup-node.sh

echo ""
echo "🚀 Starting bootstrap node (Node 1)..."
export BLIXARD_P2P_ENABLED=true
export BLIXARD_ENABLE_MDNS_DISCOVERY=false
RUST_LOG=info,blixard_core::node=debug,blixard_core::metrics_server=debug \
cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./data1 &
NODE1_PID=$!

echo "⏳ Waiting for bootstrap node to initialize..."
sleep 15

# Test bootstrap endpoint
echo ""
echo "🔍 Testing bootstrap endpoint..."
curl -s http://127.0.0.1:8001/bootstrap | jq . || echo "Bootstrap endpoint not ready yet"

echo ""
echo "🚀 Starting Node 2 (joining via P2P bootstrap)..."
export BLIXARD_P2P_ENABLED=true
export BLIXARD_ENABLE_MDNS_DISCOVERY=false
RUST_LOG=info,blixard_core::node=debug,blixard_core::transport=debug \
cargo run --bin blixard -- node --id 2 --bind 127.0.0.1:7002 --data-dir ./data2 --peers 127.0.0.1:7001 &
NODE2_PID=$!

echo "⏳ Waiting for Node 2 to join..."
sleep 10

echo ""
echo "🚀 Starting Node 3 (joining via P2P bootstrap)..."
export BLIXARD_P2P_ENABLED=true
export BLIXARD_ENABLE_MDNS_DISCOVERY=false
RUST_LOG=info,blixard_core::node=debug,blixard_core::transport=debug \
cargo run --bin blixard -- node --id 3 --bind 127.0.0.1:7003 --data-dir ./data3 --peers 127.0.0.1:7001 &
NODE3_PID=$!

echo "⏳ Waiting for cluster to form..."
sleep 10

echo ""
echo "📊 Checking cluster status..."
cargo run --bin blixard -- cluster status || echo "Cluster status check failed"

echo ""
echo "🧪 Testing VM creation on the cluster..."
cargo run --bin blixard -- vm create --name test-cluster-vm --vcpus 2 --memory 1024

echo ""
echo "📋 Listing VMs..."
cargo run --bin blixard -- vm list

echo ""
echo "🛑 Cleanup - stopping all nodes..."
kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null
wait $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null

echo ""
echo "✅ Test complete!"