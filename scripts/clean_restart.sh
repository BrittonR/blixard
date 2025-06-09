#!/bin/bash
# Clean restart script for Blixard testing

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${YELLOW}=== Cleaning up Blixard environment ===${NC}"

# Kill all BEAM processes
echo "Stopping all BEAM processes..."
killall beam.smp 2>/dev/null || true
sleep 2

# Clean up Khepri data directories
echo "Removing Khepri data directories..."
rm -rf khepri#* 2>/dev/null || true

# Clean up log files
echo "Removing log files..."
rm -f /tmp/blixard*.log 2>/dev/null || true

# Stop test service if running
echo "Stopping test-http-server..."
systemctl --user stop test-http-server 2>/dev/null || true

echo -e "${GREEN}Environment cleaned!${NC}"
echo
echo "You can now:"
echo "1. Run 'blixard-test-zellij' for a fresh test environment"
echo "2. Or manually start nodes with 'gleam run -m service_manager -- --join-cluster'"