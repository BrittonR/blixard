#!/bin/bash
# Test script to verify timing improvements and detect flaky tests
#
# This script runs tests multiple times to detect timing-related flakiness
# and can be used in CI to ensure reliability.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
TEST_ITERATIONS=${TEST_ITERATIONS:-10}
TEST_FILTER=${TEST_FILTER:-""}
FAIL_FAST=${FAIL_FAST:-false}
LOG_DIR=${LOG_DIR:-"test-timing-logs"}

# Create log directory
mkdir -p "$LOG_DIR"

echo "🧪 Running timing reliability tests"
echo "Iterations: $TEST_ITERATIONS"
echo "Test filter: ${TEST_FILTER:-all tests}"
echo "Environment: ${CI:-local}"
echo ""

# Track results
declare -A test_results
failed_tests=()
flaky_tests=()

# Function to run a single test suite
run_test_suite() {
    local suite=$1
    local iteration=$2
    local log_file="$LOG_DIR/${suite//\//_}-iter${iteration}.log"
    
    echo -n "  Iteration $iteration/$TEST_ITERATIONS... "
    
    if [[ "$suite" == "simulation" ]]; then
        # Run simulation tests with deterministic seed
        MADSIM_TEST_SEED=$iteration cargo test \
            --manifest-path simulation/Cargo.toml \
            $TEST_FILTER \
            -- --nocapture > "$log_file" 2>&1
    else
        # Run regular tests
        cargo test \
            --test "$suite" \
            $TEST_FILTER \
            -- --nocapture > "$log_file" 2>&1
    fi
    
    local exit_code=$?
    
    if [[ $exit_code -eq 0 ]]; then
        echo -e "${GREEN}✓${NC}"
        return 0
    else
        echo -e "${RED}✗${NC} (see $log_file)"
        return 1
    fi
}

# Test suites to check
test_suites=(
    "cluster_integration_tests"
    "node_tests"
    "simulation"
)

# Run each test suite multiple times
for suite in "${test_suites[@]}"; do
    echo "Testing $suite..."
    
    success_count=0
    failure_count=0
    
    for i in $(seq 1 $TEST_ITERATIONS); do
        if run_test_suite "$suite" "$i"; then
            ((success_count++))
        else
            ((failure_count++))
            if [[ "$FAIL_FAST" == "true" ]]; then
                echo -e "${RED}Failing fast on first failure${NC}"
                exit 1
            fi
        fi
    done
    
    # Record results
    test_results[$suite]="$success_count/$TEST_ITERATIONS"
    
    if [[ $failure_count -eq $TEST_ITERATIONS ]]; then
        failed_tests+=("$suite")
    elif [[ $failure_count -gt 0 ]]; then
        flaky_tests+=("$suite")
    fi
    
    echo ""
done

# Print summary
echo "📊 Test Timing Summary"
echo "====================="
echo ""

for suite in "${test_suites[@]}"; do
    result="${test_results[$suite]}"
    if [[ " ${failed_tests[@]} " =~ " ${suite} " ]]; then
        echo -e "$suite: ${RED}FAILED${NC} ($result passed)"
    elif [[ " ${flaky_tests[@]} " =~ " ${suite} " ]]; then
        echo -e "$suite: ${YELLOW}FLAKY${NC} ($result passed)"
    else
        echo -e "$suite: ${GREEN}STABLE${NC} ($result passed)"
    fi
done

echo ""

# Exit codes
if [[ ${#failed_tests[@]} -gt 0 ]]; then
    echo -e "${RED}❌ Consistently failing tests detected${NC}"
    exit 2
elif [[ ${#flaky_tests[@]} -gt 0 ]]; then
    echo -e "${YELLOW}⚠️  Flaky tests detected - timing issues may still exist${NC}"
    echo ""
    echo "Flaky tests:"
    for test in "${flaky_tests[@]}"; do
        echo "  - $test"
    done
    echo ""
    echo "To debug, check logs in $LOG_DIR/"
    exit 1
else
    echo -e "${GREEN}✅ All tests are stable!${NC}"
    exit 0
fi