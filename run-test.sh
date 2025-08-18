#!/bin/bash

# Test runner with automatic cleanup

# Source cleanup functions
source ./test-cleanup.sh

# Parse arguments
TEST_SCRIPT="$1"
if [ -z "$TEST_SCRIPT" ]; then
    echo "Usage: ./run-test.sh <test-script>"
    echo "Available tests:"
    echo "  - test-restart-fix.sh"
    echo "  - test-node-failure.sh"
    echo "  - test-cluster-health.sh"
    exit 1
fi

if [ ! -f "$TEST_SCRIPT" ]; then
    echo "Error: Test script '$TEST_SCRIPT' not found"
    exit 1
fi

echo "=== RUNNING TEST: $TEST_SCRIPT ==="
echo ""

# Pre-test cleanup
echo "Pre-test cleanup..."
full_cleanup
echo ""

# Run the test
bash "$TEST_SCRIPT"
TEST_EXIT_CODE=$?

# Post-test cleanup (only if test didn't do it)
echo ""
echo "Post-test cleanup..."
cleanup_all_nodes

echo ""
if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo "✅ TEST PASSED"
else
    echo "❌ TEST FAILED (exit code: $TEST_EXIT_CODE)"
fi

exit $TEST_EXIT_CODE