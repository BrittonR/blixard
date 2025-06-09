#!/bin/bash
# Test script to debug the stop service issue

set -e

echo "=== Testing Blixard Stop Service Issue ==="

# Clean any existing data
rm -rf khepri#*

# 1. Start cluster node
echo "Starting cluster node..."
gleam run -m service_manager -- --join-cluster &
CLUSTER_PID=$!
sleep 5

# 2. Create a simple test service first
echo -e "\n--- Creating test service ---"
mkdir -p ~/.config/systemd/user/
cat > ~/.config/systemd/user/test-service.service << 'EOF'
[Unit]
Description=Test Service for Blixard

[Service]
Type=oneshot
ExecStart=/bin/echo "Test service executed"
RemainAfterExit=yes

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload

# 3. Start the test service  
echo -e "\n--- Starting test service ---"
gleam run -m service_manager -- start --user test-service

# 3. List services to confirm it's managed
echo -e "\n--- Listing services ---"
gleam run -m service_manager -- list

# 4. Stop the service
echo -e "\n--- Stopping test service ---"
gleam run -m service_manager -- stop --user test-service

# 5. List again to see if it's marked as stopped
echo -e "\n--- Listing services after stop ---"
gleam run -m service_manager -- list

# Cleanup
echo -e "\n--- Cleanup ---"
kill $CLUSTER_PID 2>/dev/null || true
systemctl --user stop test-service 2>/dev/null || true
rm -f ~/.config/systemd/user/test-service.service
systemctl --user daemon-reload