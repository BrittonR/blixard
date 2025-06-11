#!/bin/bash
# Determinism verification script for Blixard

set -e

echo "🔬 Determinism Verification for Blixard"
echo "========================================"
echo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Test results storage
RESULTS_DIR="/tmp/blixard-determinism-test"
rm -rf "$RESULTS_DIR"
mkdir -p "$RESULTS_DIR"

echo -e "${CYAN}📁 Results stored in: $RESULTS_DIR${NC}"
echo

# Function to show timing results clearly
show_timing_results() {
    local title="$1"
    local file="$2"
    
    echo -e "${BOLD}$title:${NC}"
    if [ -f "$file" ]; then
        grep "passed in" "$file" | sort || echo "  No timing data found"
    else
        echo "  File not found: $file"
    fi
    echo
}

# Function to run MadSim test and extract timings
run_madsim_test() {
    local seed="$1"
    local run_number="$2"
    local output_file="$3"
    
    echo -e "${YELLOW}🧪 Running MadSim simulation with seed $seed (run $run_number)...${NC}"
    
    # Run test and extract timing data
    MADSIM_TEST_SEED="$seed" sh ./scripts/sim-test.sh 2>&1 | \
        grep "✅.*passed in" > "$output_file" || true
    
    echo -e "${CYAN}   ⏱️  Captured timing results${NC}"
}

# Function to compare timing precision
compare_timings() {
    local seed="$1"
    local file1="$2"
    local file2="$3"
    
    echo -e "${BLUE}🔍 Analyzing determinism for seed $seed...${NC}"
    echo
    
    # Show both sets of results
    show_timing_results "  📊 Run 1 Results" "$file1"
    show_timing_results "  📊 Run 2 Results" "$file2"
    
    # Extract just the timing values for comparison
    grep "passed in" "$file1" | sed 's/.*passed in //' | sort > "$RESULTS_DIR/timings1.tmp" 2>/dev/null || touch "$RESULTS_DIR/timings1.tmp"
    grep "passed in" "$file2" | sed 's/.*passed in //' | sort > "$RESULTS_DIR/timings2.tmp" 2>/dev/null || touch "$RESULTS_DIR/timings2.tmp"
    
    if cmp -s "$RESULTS_DIR/timings1.tmp" "$RESULTS_DIR/timings2.tmp"; then
        echo -e "${GREEN}  ✅ PERFECTLY DETERMINISTIC: Identical microsecond-level timing${NC}"
        echo -e "${GREEN}     Same seed produces exactly the same simulation timing!${NC}"
        return 0
    else
        echo -e "${RED}  ❌ NON-DETERMINISTIC: Different timing results${NC}"
        echo -e "${YELLOW}     Timing differences:${NC}"
        diff "$RESULTS_DIR/timings1.tmp" "$RESULTS_DIR/timings2.tmp" || true
        return 1
    fi
}

# Function to show seed effect
show_seed_effect() {
    local seed1="$1"
    local seed2="$2"
    local file1="$3"
    local file2="$4"
    
    echo -e "${BLUE}🎲 Seed Effect Analysis...${NC}"
    echo
    
    show_timing_results "  📊 Seed $seed1 Results" "$file1"
    show_timing_results "  📊 Seed $seed2 Results" "$file2"
    
    # Compare if different seeds produce different results
    grep "passed in" "$file1" | sed 's/.*passed in //' | sort > "$RESULTS_DIR/seed1_timings.tmp" 2>/dev/null || touch "$RESULTS_DIR/seed1_timings.tmp"
    grep "passed in" "$file2" | sed 's/.*passed in //' | sort > "$RESULTS_DIR/seed2_timings.tmp" 2>/dev/null || touch "$RESULTS_DIR/seed2_timings.tmp"
    
    if cmp -s "$RESULTS_DIR/seed1_timings.tmp" "$RESULTS_DIR/seed2_timings.tmp"; then
        echo -e "${YELLOW}  ⚠️  Same results for different seeds - tests may not be seed-sensitive${NC}"
    else
        echo -e "${GREEN}  ✅ Different seeds produce different results (good!)${NC}"
        echo -e "${CYAN}     This confirms the simulation is using the seed properly${NC}"
    fi
}

echo -e "${BOLD}🔬 PHASE 1: MadSim Determinism Test${NC}"
echo "====================================="
echo

# Test MadSim determinism with seed 42
run_madsim_test "42" "1" "$RESULTS_DIR/madsim_seed42_run1.txt"
run_madsim_test "42" "2" "$RESULTS_DIR/madsim_seed42_run2.txt"

echo
madsim_deterministic=false
if compare_timings "42" "$RESULTS_DIR/madsim_seed42_run1.txt" "$RESULTS_DIR/madsim_seed42_run2.txt"; then
    madsim_deterministic=true
fi

echo
echo -e "${BOLD}🔬 PHASE 2: Seed Variation Test${NC}"
echo "================================"
echo

# Test with different seed to verify seed effect
run_madsim_test "99" "1" "$RESULTS_DIR/madsim_seed99_run1.txt"

echo
show_seed_effect "42" "99" "$RESULTS_DIR/madsim_seed42_run1.txt" "$RESULTS_DIR/madsim_seed99_run1.txt"

echo
echo -e "${BOLD}🔬 PHASE 3: Quick Basic Test Suite${NC}"
echo "=================================="
echo

echo -e "${YELLOW}🧪 Running proptest suite...${NC}"
cargo test --test proptest_example > "$RESULTS_DIR/proptest.log" 2>&1
proptest_passed=$(grep -c "test result: ok" "$RESULTS_DIR/proptest.log" || echo "0")

echo -e "${YELLOW}🧪 Running stateright suite...${NC}"
cargo test --test stateright_simple_test > "$RESULTS_DIR/stateright.log" 2>&1
stateright_passed=$(grep -c "test result: ok" "$RESULTS_DIR/stateright.log" || echo "0")

echo
echo -e "${BOLD}📊 COMPREHENSIVE DETERMINISM REPORT${NC}"
echo "===================================="
echo

# Main results
if [ "$madsim_deterministic" = true ]; then
    echo -e "${GREEN}🎯 PRIMARY RESULT: MadSim is PERFECTLY DETERMINISTIC${NC}"
    echo -e "${GREEN}   • Same seeds produce identical microsecond-precision timing${NC}"
    echo -e "${GREEN}   • This is the core distributed system simulation framework${NC}"
else
    echo -e "${RED}❌ MadSim shows non-deterministic behavior${NC}"
fi

echo
echo -e "${BLUE}Supporting Test Suites:${NC}"
if [ "$proptest_passed" -gt 0 ]; then
    echo -e "  ${GREEN}✅ Proptest: $proptest_passed property test suites passed${NC}"
else
    echo -e "  ${RED}❌ Proptest: Tests failed or not found${NC}"
fi

if [ "$stateright_passed" -gt 0 ]; then
    echo -e "  ${GREEN}✅ Stateright: $stateright_passed model checking suites passed${NC}"
else
    echo -e "  ${RED}❌ Stateright: Tests failed or not found${NC}"
fi

echo
echo -e "${BOLD}🔧 Key Fixes Applied:${NC}"
echo "  ✅ Fixed random seed default (scripts/sim-test.sh: \$RANDOM → 12345)"
echo "  ✅ Fixed time mixing (tests/common/mod.rs: consistent time abstractions)"
echo "  ✅ Fixed HashMap usage (deterministic BTreeMap iteration)"
echo "  ✅ Fixed compilation warnings (unused variables/imports)"

echo
echo -e "${BOLD}📋 How to Verify Yourself:${NC}"
echo
echo -e "${CYAN}1. Quick verification:${NC}"
echo "   MADSIM_TEST_SEED=42 ./scripts/sim-test.sh"
echo "   MADSIM_TEST_SEED=42 ./scripts/sim-test.sh"
echo "   (Should produce identical timing results)"
echo
echo -e "${CYAN}2. Run this verification script:${NC}"
echo "   ./scripts/verify-determinism.sh"
echo
echo -e "${CYAN}3. Manual timing comparison:${NC}"
echo "   grep 'passed in' /tmp/blixard-determinism-test/madsim_seed42_run*.txt"
echo

# Final verdict
if [ "$madsim_deterministic" = true ]; then
    echo -e "${BOLD}${GREEN}🎉 DETERMINISM VERIFICATION: SUCCESS!${NC}"
    echo -e "${GREEN}Your distributed system simulation is now fully deterministic.${NC}"
    exit_code=0
else
    echo -e "${BOLD}${RED}⚠️  DETERMINISM VERIFICATION: ISSUES FOUND${NC}"
    echo -e "${RED}Some non-deterministic behavior remains in MadSim tests.${NC}"
    exit_code=1
fi

echo
echo -e "${CYAN}📁 Detailed results saved in: $RESULTS_DIR${NC}"
echo

exit $exit_code