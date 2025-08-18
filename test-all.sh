#!/bin/bash

# Run all cluster tests with proper cleanup between each

# Source cleanup functions
source ./test-cleanup.sh

echo "=== RUNNING ALL CLUSTER TESTS ==="
echo ""

# Track test results
PASSED=0
FAILED=0

# Test 1: Restart fix test
echo "1. Running restart fix test..."
bash ./run-test.sh test-restart-fix.sh
if [ $? -eq 0 ]; then
    echo "   ✅ Restart fix test PASSED"
    PASSED=$((PASSED + 1))
else
    echo "   ❌ Restart fix test FAILED"
    FAILED=$((FAILED + 1))
fi
echo ""

# Test 2: Node failure test
echo "2. Running node failure test..."
bash ./run-test.sh test-node-failure.sh
if [ $? -eq 0 ]; then
    echo "   ✅ Node failure test PASSED"
    PASSED=$((PASSED + 1))
else
    echo "   ❌ Node failure test FAILED"
    FAILED=$((FAILED + 1))
fi
echo ""

# Final cleanup
echo "Final cleanup..."
full_cleanup

echo ""
echo "=== TEST SUITE RESULTS ==="
echo "Passed: $PASSED"
echo "Failed: $FAILED"

if [ $FAILED -eq 0 ]; then
    echo "🎉 ALL TESTS PASSED!"
    exit 0
else
    echo "⚠️  SOME TESTS FAILED"
    exit 1
fi