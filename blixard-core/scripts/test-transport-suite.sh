#!/bin/bash
# Automated test suite for transport switching and migration

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Transport Test Suite ==="
echo "Testing transport switching and migration capabilities"
echo

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to run a test and report results
run_test() {
    local test_name=$1
    local test_command=$2
    
    echo -n "Running $test_name... "
    
    if $test_command > /tmp/test_output.log 2>&1; then
        echo -e "${GREEN}✓ PASSED${NC}"
        return 0
    else
        echo -e "${RED}✗ FAILED${NC}"
        echo "  Error output:"
        tail -20 /tmp/test_output.log | sed 's/^/    /'
        return 1
    fi
}

# Track test results
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

echo "1. Unit Tests - Transport Configuration"
echo "======================================="

# Transport switching tests
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if run_test "transport_switching_test" "cargo test --test transport_switching_test --features test-helpers -- --nocapture"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
else
    FAILED_TESTS=$((FAILED_TESTS + 1))
fi

# Transport migration tests
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if run_test "transport_migration_test" "cargo test --test transport_migration_test --features test-helpers -- --nocapture"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
else
    FAILED_TESTS=$((FAILED_TESTS + 1))
fi

echo
echo "2. Integration Tests - Iroh Transport"
echo "====================================="

# Iroh integration tests
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if run_test "iroh_transport_integration" "cargo test --test iroh_transport_integration_test --features test-helpers -- --nocapture"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
else
    FAILED_TESTS=$((FAILED_TESTS + 1))
fi

echo
echo "3. Examples - Transport Validation"
echo "=================================="

# Build all transport examples
examples=(
    "iroh_basic_test"
    "iroh_three_node_test"
    "iroh_nat_traversal_test"
    "iroh_raft_integration_test"
)

for example in "${examples[@]}"; do
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    if run_test "build_$example" "cargo build --example $example --features test-helpers"; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
done

echo
echo "4. Transport Benchmarks"
echo "======================"

# Run benchmarks (don't fail on benchmark results)
echo -e "${YELLOW}Running benchmarks (informational only)...${NC}"
cargo run --example iroh_raft_benchmark 2>/dev/null || true
cargo run --example iroh_network_test 2>/dev/null || true

echo
echo "5. Configuration Validation"
echo "=========================="

# Check transport configurations
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if run_test "config_serialization" "cargo test transport_config_serialization --features test-helpers"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
else
    FAILED_TESTS=$((FAILED_TESTS + 1))
fi

echo
echo "======================================="
echo "Transport Test Suite Summary"
echo "======================================="
echo "Total Tests: $TOTAL_TESTS"
echo -e "Passed: ${GREEN}$PASSED_TESTS${NC}"
echo -e "Failed: ${RED}$FAILED_TESTS${NC}"
echo

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}✅ All transport tests passed!${NC}"
    echo
    echo "Transport switching capabilities validated:"
    echo "  - gRPC transport ✓"
    echo "  - Iroh P2P transport ✓"
    echo "  - Dual mode operation ✓"
    echo "  - Migration strategies ✓"
    echo "  - Service-based routing ✓"
    exit 0
else
    echo -e "${RED}❌ Some tests failed${NC}"
    echo
    echo "Please check the failed tests above for details."
    exit 1
fi