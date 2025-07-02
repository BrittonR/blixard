#!/bin/bash
# Comprehensive Test Suite for Blixard
# Runs all testing frameworks with proper configuration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
SEED=${SEED:-12345}
PROPTEST_CASES=${PROPTEST_CASES:-100}
VOPR_DURATION=${VOPR_DURATION:-60}
ENABLE_FAILPOINTS=${ENABLE_FAILPOINTS:-true}

echo -e "${BLUE}=== Blixard Comprehensive Test Suite ===${NC}"
echo "Seed: $SEED"
echo "PropTest cases: $PROPTEST_CASES"
echo "VOPR duration: ${VOPR_DURATION}s"
echo "Failpoints enabled: $ENABLE_FAILPOINTS"
echo

# Function to run a test category
run_test_category() {
    local category=$1
    local command=$2
    
    echo -e "${YELLOW}Running $category tests...${NC}"
    
    if eval "$command"; then
        echo -e "${GREEN}✓ $category tests passed${NC}"
        return 0
    else
        echo -e "${RED}✗ $category tests failed${NC}"
        return 1
    fi
}

# Track failures
FAILED_TESTS=()

# 1. Unit Tests
if ! run_test_category "Unit" "cargo test --features test-helpers --lib"; then
    FAILED_TESTS+=("Unit")
fi

# 2. Property Tests with increased cases
export PROPTEST_CASES
if ! run_test_category "Property" "cargo test --features test-helpers proptest"; then
    FAILED_TESTS+=("Property")
fi

# 3. Linearizability Tests
if ! run_test_category "Linearizability" "cargo test --features test-helpers linearizability"; then
    FAILED_TESTS+=("Linearizability")
fi

# 4. Raft Safety Properties
if ! run_test_category "Raft Safety" "cargo test --features test-helpers raft_safety_properties"; then
    FAILED_TESTS+=("Raft Safety")
fi

# 5. Byzantine Resilience
if ! run_test_category "Byzantine" "cargo test --features test-helpers raft_byzantine_properties"; then
    FAILED_TESTS+=("Byzantine")
fi

# 6. Deterministic Replay
if ! run_test_category "Deterministic Replay" "cargo test --features test-helpers raft_deterministic_replay"; then
    FAILED_TESTS+=("Deterministic Replay")
fi

# 7. MadSim Simulation Tests
export MADSIM_TEST_SEED=$SEED
if ! run_test_category "MadSim Simulation" "RUSTFLAGS='--cfg madsim' cargo test -p simulation"; then
    FAILED_TESTS+=("MadSim Simulation")
fi

# 8. Failpoint Tests
if [ "$ENABLE_FAILPOINTS" = "true" ]; then
    if ! run_test_category "Failpoint" "cargo test --features 'test-helpers failpoints' failpoint"; then
        FAILED_TESTS+=("Failpoint")
    fi
fi

# 9. VOPR Fuzzer (short run for CI)
export VOPR_SEED=$SEED
export VOPR_DURATION
if ! run_test_category "VOPR Fuzzer" "timeout ${VOPR_DURATION}s cargo run --example vopr_demo --features vopr -- $SEED || [ $? -eq 124 ]"; then
    FAILED_TESTS+=("VOPR Fuzzer")
fi

# 10. Integration Tests
if ! run_test_category "Integration" "cargo test --features test-helpers --test '*'"; then
    FAILED_TESTS+=("Integration")
fi

# Summary
echo
echo -e "${BLUE}=== Test Summary ===${NC}"

if [ ${#FAILED_TESTS[@]} -eq 0 ]; then
    echo -e "${GREEN}All test categories passed!${NC}"
    exit 0
else
    echo -e "${RED}Failed test categories:${NC}"
    for test in "${FAILED_TESTS[@]}"; do
        echo -e "  ${RED}✗ $test${NC}"
    done
    exit 1
fi