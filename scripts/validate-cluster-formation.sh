#!/bin/bash
# Validate cluster formation by running tests multiple times

set -e

echo "=== Validating Cluster Formation ==="
echo

# Number of iterations
ITERATIONS=10

# Track results
PASSED=0
FAILED=0

echo "Running cluster formation tests $ITERATIONS times..."
echo

for i in $(seq 1 $ITERATIONS); do
    echo -n "Run $i/$ITERATIONS: "
    
    if cargo test cluster_formation --features test-helpers --release 2>/dev/null | grep -q "test result: ok"; then
        echo "✓ PASSED"
        ((PASSED++))
    else
        echo "✗ FAILED"
        ((FAILED++))
    fi
done

echo
echo "=== Results ==="
echo "Passed: $PASSED/$ITERATIONS"
echo "Failed: $FAILED/$ITERATIONS"

if [ $FAILED -eq 0 ]; then
    echo
    echo "✅ All tests passed! Cluster formation appears stable."
    exit 0
else
    echo
    echo "❌ Some tests failed. Cluster formation may be flaky."
    exit 1
fi