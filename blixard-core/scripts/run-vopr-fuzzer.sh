#!/bin/bash
# Script to run the VOPR fuzzer with various configurations

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🚀 Blixard VOPR Fuzzer Runner${NC}"
echo "==============================="
echo

# Default values
SEED=${1:-$RANDOM}
OPERATIONS=${2:-1000}
FEATURE_FLAGS="--features vopr"

echo -e "${YELLOW}Configuration:${NC}"
echo "  Seed: $SEED"
echo "  Max operations: $OPERATIONS"
echo "  Features: vopr"
echo

# Run tests first
echo -e "${YELLOW}Running VOPR tests...${NC}"
if cargo test $FEATURE_FLAGS vopr_test -- --nocapture; then
    echo -e "${GREEN}✓ Tests passed${NC}"
else
    echo -e "${RED}✗ Tests failed${NC}"
    exit 1
fi

echo
echo -e "${YELLOW}Running VOPR fuzzer demo...${NC}"

# Run the fuzzer
if cargo run --example vopr_demo $FEATURE_FLAGS -- $SEED; then
    echo -e "${GREEN}✓ Fuzzing completed successfully${NC}"
else
    echo -e "${RED}✗ Fuzzing found issues${NC}"
    echo -e "${YELLOW}Check vopr_failure_${SEED}.txt for details${NC}"
fi

echo
echo -e "${YELLOW}To reproduce a specific failure:${NC}"
echo "  cargo run --example vopr_demo $FEATURE_FLAGS -- <seed>"
echo
echo -e "${YELLOW}To run with custom settings, edit examples/vopr_demo.rs${NC}"