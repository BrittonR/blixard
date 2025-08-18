#!/bin/bash

# Shared cleanup functions for test scripts

cleanup_all_nodes() {
    echo "Cleaning up all Blixard processes..."
    
    # Kill all blixard processes
    pkill -f "target/debug/blixard" 2>/dev/null || true
    
    # Wait a moment for processes to terminate
    sleep 1
    
    # Force kill if any remain
    pkill -9 -f "target/debug/blixard" 2>/dev/null || true
    
    # Verify cleanup
    REMAINING=$(ps aux | grep "target/debug/blixard" | grep -v grep | wc -l)
    if [ "$REMAINING" -eq 0 ]; then
        echo "✅ All Blixard processes terminated"
    else
        echo "⚠️  Warning: $REMAINING Blixard processes still running"
        ps aux | grep "target/debug/blixard" | grep -v grep
    fi
}

cleanup_logs() {
    echo "Cleaning up log files..."
    rm -f node*.log 2>/dev/null
    rm -f node*_restart.log 2>/dev/null
    echo "✅ Log files cleaned"
}

cleanup_data() {
    echo "Cleaning up data directories..."
    rm -rf test_node1 test_node2 test_node3 2>/dev/null
    echo "✅ Data directories cleaned"
}

full_cleanup() {
    echo "=== FULL TEST CLEANUP ==="
    cleanup_all_nodes
    cleanup_logs
    cleanup_data
    echo "=== CLEANUP COMPLETE ==="
    echo ""
}

# If sourced with argument, run the requested cleanup
if [ "$1" = "all" ]; then
    cleanup_all_nodes
elif [ "$1" = "logs" ]; then
    cleanup_logs
elif [ "$1" = "data" ]; then
    cleanup_data
elif [ "$1" = "full" ]; then
    full_cleanup
fi