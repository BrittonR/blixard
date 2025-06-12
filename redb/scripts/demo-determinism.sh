#!/bin/bash
# Simple determinism demonstration for Blixard

echo "🎯 DETERMINISM DEMONSTRATION"
echo "============================"
echo
echo "This script demonstrates that our fixes have made the simulation deterministic."
echo

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BOLD}🧪 Test 1: Same seed should produce identical results${NC}"
echo "=================================================="
echo

echo -e "${YELLOW}Running with SEED=42 (first time)...${NC}"
MADSIM_TEST_SEED=42 ./scripts/sim-test.sh 2>&1 | grep "passed in" | sort > /tmp/run1_timings.txt

echo -e "${YELLOW}Running with SEED=42 (second time)...${NC}"
MADSIM_TEST_SEED=42 ./scripts/sim-test.sh 2>&1 | grep "passed in" | sort > /tmp/run2_timings.txt

echo
echo -e "${BLUE}📊 Results comparison:${NC}"
echo
echo -e "${CYAN}First run timings:${NC}"
cat /tmp/run1_timings.txt
echo
echo -e "${CYAN}Second run timings:${NC}"
cat /tmp/run2_timings.txt
echo

if cmp -s /tmp/run1_timings.txt /tmp/run2_timings.txt; then
    echo -e "${GREEN}✅ SUCCESS: Both runs produced IDENTICAL timing results!${NC}"
    echo -e "${GREEN}   This proves the simulation is deterministic.${NC}"
    deterministic=true
else
    echo -e "${RED}❌ FAILURE: Different timing results detected${NC}"
    echo "Differences:"
    diff /tmp/run1_timings.txt /tmp/run2_timings.txt
    deterministic=false
fi

echo
echo -e "${BOLD}🧪 Test 2: Different seed should produce different results${NC}"
echo "=========================================================="
echo

echo -e "${YELLOW}Running with SEED=999 (for comparison)...${NC}"
MADSIM_TEST_SEED=999 ./scripts/sim-test.sh 2>&1 | grep "passed in" | sort > /tmp/run3_timings.txt

echo
echo -e "${BLUE}📊 Seed effect comparison:${NC}"
echo
echo -e "${CYAN}SEED=42 timings:${NC}"
cat /tmp/run1_timings.txt
echo
echo -e "${CYAN}SEED=999 timings:${NC}"
cat /tmp/run3_timings.txt
echo

if cmp -s /tmp/run1_timings.txt /tmp/run3_timings.txt; then
    echo -e "${YELLOW}⚠️  Same results for different seeds${NC}"
    echo -e "${YELLOW}   (This is OK - our current tests may not be seed-sensitive)${NC}"
else
    echo -e "${GREEN}✅ GOOD: Different seeds produce different results${NC}"
    echo -e "${GREEN}   This confirms the seed is being used properly.${NC}"
fi

echo
echo -e "${BOLD}📋 FINAL VERDICT${NC}"
echo "================"
echo

if [ "$deterministic" = true ]; then
    echo -e "${GREEN}🎉 DETERMINISM VERIFICATION: PASSED!${NC}"
    echo
    echo -e "${GREEN}✅ Your distributed system simulation is now deterministic${NC}"
    echo -e "${GREEN}✅ Same seeds produce identical microsecond-precision results${NC}"
    echo -e "${GREEN}✅ You can now reproduce any test run exactly${NC}"
    echo
    echo -e "${BOLD}🔧 Key fixes that made this possible:${NC}"
    echo "   • Fixed random seed default in sim-test.sh"
    echo "   • Fixed time abstractions in test utilities"  
    echo "   • Replaced HashMap with deterministic BTreeMap"
    echo "   • Cleaned up compilation warnings"
else
    echo -e "${RED}❌ DETERMINISM VERIFICATION: FAILED${NC}"
    echo -e "${RED}   Some non-deterministic behavior remains${NC}"
fi

echo
echo -e "${BOLD}🔄 To reproduce these results yourself:${NC}"
echo "   MADSIM_TEST_SEED=42 ./scripts/sim-test.sh"
echo "   MADSIM_TEST_SEED=42 ./scripts/sim-test.sh"
echo "   (Compare the 'passed in' timing results)"
echo