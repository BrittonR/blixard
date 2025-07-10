#!/bin/bash
# Unwrap Analysis and Progress Tracking Script
# Usage: ./scripts/unwrap-analysis.sh [command]
# Commands: count, progress, high-risk, test-vs-prod

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BLUE}=== $1 ===${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

count_unwraps() {
    print_header "Total Unwrap Analysis"
    
    echo "Analyzing unwrap() calls across the codebase..."
    
    # Total count
    local total=$(rg '\.unwrap\(\)' "$PROJECT_ROOT" --type rust -c | awk -F: '{sum+=$2} END {print sum+0}')
    
    # Production code count (excluding tests, examples, benches)
    local production=$(rg '\.unwrap\(\)' "$PROJECT_ROOT" --type rust -c | \
        grep -v '/tests/' | grep -v '/test_' | grep -v '/examples/' | grep -v '/benches/' | \
        awk -F: '{sum+=$2} END {print sum+0}')
    
    # Test code count
    local test_count=$((total - production))
    
    echo ""
    echo "📊 UNWRAP DISTRIBUTION:"
    echo "  Total unwrap() calls: $total"
    echo "  Production code: $production ($(( production * 100 / total ))%)"
    echo "  Test code: $test_count ($(( test_count * 100 / total ))%)"
    
    if [ "$production" -gt 100 ]; then
        print_error "High production unwrap count ($production) - critical stability risk!"
    elif [ "$production" -gt 50 ]; then
        print_warning "Moderate production unwrap count ($production)"
    else
        print_success "Low production unwrap count ($production)"
    fi
}

show_progress() {
    print_header "Implementation Progress"
    
    # Get current counts for key files
    local files=(
        "blixard-core/src/iroh_transport_v2.rs"
        "blixard-core/src/ip_pool_manager.rs" 
        "blixard-core/src/iroh_transport_v3.rs"
        "blixard-core/src/resource_admission.rs"
        "blixard-core/src/ip_pool.rs"
        "blixard-core/src/database_transaction.rs"
    )
    
    echo "📈 PILOT FIXES PROGRESS:"
    
    for file in "${files[@]}"; do
        if [ -f "$PROJECT_ROOT/$file" ]; then
            local count=$(rg '\.unwrap\(\)' "$PROJECT_ROOT/$file" -c 2>/dev/null || echo "0")
            local status=""
            case "$file" in
                *ip_pool_manager.rs) status="✅ PILOT FIXED (35→$count)" ;;
                *resource_admission.rs) status="✅ PILOT FIXED (27→$count)" ;;
                *iroh_transport*) status="⚠️  CRITICAL PRIORITY" ;;
                *) status="📋 Pending" ;;
            esac
            printf "  %-40s %3s calls %s\n" "$(basename "$file")" "$count" "$status"
        fi
    done
    
    echo ""
    echo "🎯 PHASE TARGETS:"
    echo "  Phase 1 (Critical):    107 unwraps in transport/consensus"
    echo "  Phase 2 (High):        162 unwraps in resource management"  
    echo "  Phase 3 (Medium):      150 unwraps in security/lifecycle"
    echo "  Phase 4 (Low):         243 unwraps in utilities"
    echo "  Phase 5 (Optional):   2445 unwraps in test code"
}

show_high_risk() {
    print_header "High-Risk Production Files"
    
    echo "🚨 TOP 10 PRODUCTION FILES BY UNWRAP COUNT:"
    rg '\.unwrap\(\)' "$PROJECT_ROOT/blixard-core/src" --type rust -c | \
        grep -v '/tests/' | grep -v '/test_' | grep -v '/examples/' | grep -v '/benches/' | \
        sort -t: -k2 -nr | head -10 | while IFS=: read -r file count; do
            local basename=$(basename "$file")
            local priority="📋"
            case "$basename" in
                iroh_transport*) priority="🔥 CRITICAL" ;;
                *pool*|*admission*|*database*) priority="⚠️  HIGH" ;;
                *cert*|*quota*|*resource*) priority="📊 MEDIUM" ;;
            esac
            printf "  %-40s %3s calls %s\n" "$basename" "$count" "$priority"
        done
    
    echo ""
    print_warning "Focus Phase 1 effort on CRITICAL and HIGH priority files"
}

test_vs_prod_breakdown() {
    print_header "Test vs Production Breakdown"
    
    echo "🔍 DETAILED BREAKDOWN BY CATEGORY:"
    
    echo ""
    echo "Production code files:"
    rg '\.unwrap\(\)' "$PROJECT_ROOT/blixard-core/src" --type rust -c | \
        grep -v '/tests/' | grep -v '/test_' | head -5 | while IFS=: read -r file count; do
            printf "  %s: %s calls\n" "$(basename "$file")" "$count"
        done
    
    echo ""
    echo "Test code files:"
    rg '\.unwrap\(\)' "$PROJECT_ROOT/blixard-core/tests" --type rust -c 2>/dev/null | \
        head -5 | while IFS=: read -r file count; do
            printf "  %s: %s calls\n" "$(basename "$file")" "$count"
        done || echo "  (No test directory found)"
    
    echo ""
    echo "💡 RECOMMENDATION: Focus on production files first - test unwraps are generally acceptable"
}

show_help() {
    echo "Unwrap Analysis Script"
    echo ""
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  count       Show total unwrap counts and distribution"
    echo "  progress    Show implementation progress on pilot fixes"
    echo "  high-risk   Show highest-risk production files"
    echo "  test-vs-prod Show breakdown of test vs production unwraps"
    echo "  help        Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 count           # Quick overview"
    echo "  $0 high-risk       # Identify next files to fix"
    echo "  $0 progress        # Track pilot implementation status"
}

# Main command handling
case "${1:-count}" in
    count)
        count_unwraps
        ;;
    progress)
        show_progress
        ;;
    high-risk)
        show_high_risk
        ;;
    test-vs-prod)
        test_vs_prod_breakdown
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        echo "Unknown command: $1"
        echo ""
        show_help
        exit 1
        ;;
esac