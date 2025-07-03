#\!/bin/bash
# Simple P2P bootstrap test

echo "=== P2P Bootstrap Test ==="
echo ""

# Clean up
echo "🧹 Cleaning up..."
pkill -f "blixard.*node" || true
rm -rf ./test-data-*
sleep 2

# Start node 1
echo "🚀 Starting Node 1..."
export BLIXARD_P2P_ENABLED=true
export BLIXARD_ENABLE_MDNS_DISCOVERY=false
RUST_LOG=info,blixard_core::node=debug,blixard_core::metrics_server=debug \
cargo run --bin blixard -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./test-data-1 &
NODE1_PID=$\!

echo "⏳ Waiting for Node 1 to start..."
sleep 15

# Test bootstrap endpoint
echo ""
echo "🔍 Testing bootstrap endpoint..."
curl -s http://127.0.0.1:8001/bootstrap  < /dev/null |  jq . || echo "Failed to get bootstrap info"

echo ""
echo "🚀 Starting Node 2..."
export BLIXARD_P2P_ENABLED=true
export BLIXARD_ENABLE_MDNS_DISCOVERY=false
RUST_LOG=info,blixard_core::node=debug,blixard_core::transport=debug \
cargo run --bin blixard -- node --id 2 --bind 127.0.0.1:7002 --data-dir ./test-data-2 --peers 127.0.0.1:7001 &
NODE2_PID=$!

echo "⏳ Waiting for Node 2 to join..."
sleep 15

echo ""
echo "📊 Checking cluster status..."
# Wait a bit more then check status
sleep 5
cargo run --bin blixard -- cluster status

echo ""
echo "🛑 Stopping nodes..."
kill $NODE1_PID $NODE2_PID 2>/dev/null

echo ""
echo "✅ Test complete!"
