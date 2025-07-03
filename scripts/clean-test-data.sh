#!/bin/bash
# Clean up test data directories and processes

echo "Cleaning up test data..."

# Kill any running blixard processes
pkill -f "blixard node" || true

# Remove test data directories
rm -rf ./data-node-1 ./data-node-2 ./data-node-3

# Give processes time to fully exit
sleep 1

echo "Cleanup complete"