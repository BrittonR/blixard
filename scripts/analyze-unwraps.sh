#!/bin/bash
# analyze-unwraps.sh - Find and analyze unwrap() usage in the codebase

set -euo pipefail

echo "🔍 Analyzing unwrap() usage in Blixard codebase..."
echo "================================================"

# Function to display top offenders
show_top_offenders() {
    echo -e "\n📊 Top 20 files with most unwrap() calls:"
    echo "----------------------------------------"
    rg "\.unwrap\(\)" --type rust -c | sort -t: -k2 -nr | head -20 | while IFS=: read -r file count; do
        printf "%-50s %3d unwraps\n" "$file" "$count"
    done
}

# Function to analyze by component
analyze_by_component() {
    echo -e "\n📦 Unwrap usage by component:"
    echo "-----------------------------"
    
    echo -e "\nCore components:"
    rg "\.unwrap\(\)" --type rust -c blixard-core/src/ 2>/dev/null | wc -l | xargs echo "  blixard-core: "
    
    echo -e "\nVM subsystem:"
    rg "\.unwrap\(\)" --type rust -c blixard-vm/src/ 2>/dev/null | wc -l | xargs echo "  blixard-vm: "
    
    echo -e "\nTransport layer:"
    rg "\.unwrap\(\)" --type rust -c blixard-core/src/transport/ 2>/dev/null | wc -l | xargs echo "  transport: "
}

# Function to find critical unwraps
find_critical_unwraps() {
    echo -e "\n⚠️  Critical unwrap() patterns:"
    echo "------------------------------"
    
    echo -e "\nNetwork operations with unwrap:"
    rg "\.unwrap\(\)" --type rust -B2 -A0 | grep -B2 "TcpListener\|connect\|bind" | head -10 || echo "  None found"
    
    echo -e "\nFile operations with unwrap:"
    rg "\.unwrap\(\)" --type rust -B2 -A0 | grep -B2 "File::\|read_to_string\|write" | head -10 || echo "  None found"
    
    echo -e "\nDatabase operations with unwrap:"
    rg "\.unwrap\(\)" --type rust -B2 -A0 | grep -B2 "transaction\|query\|execute" | head -10 || echo "  None found"
}

# Function to generate fix suggestions
generate_fix_suggestions() {
    echo -e "\n💡 Suggested fixes for common patterns:"
    echo "--------------------------------------"
    
    echo "1. Environment variables:"
    echo "   Before: env::var(\"PORT\").unwrap()"
    echo "   After:  env::var(\"PORT\").unwrap_or_else(|_| \"8080\".to_string())"
    
    echo -e "\n2. Parsing:"
    echo "   Before: value.parse().unwrap()"
    echo "   After:  value.parse().context(\"Failed to parse value\")?"
    
    echo -e "\n3. Lock acquisition:"
    echo "   Before: mutex.lock().unwrap()"
    echo "   After:  mutex.lock().map_err(|_| BlixardError::LockPoisoned)?"
}

# Function to track progress
track_progress() {
    echo -e "\n📈 Progress tracking:"
    echo "-------------------"
    
    total=$(rg "\.unwrap\(\)" --type rust -c | awk -F: '{sum += $2} END {print sum}')
    files=$(rg "\.unwrap\(\)" --type rust -c | wc -l)
    
    echo "Total unwrap() calls: $total"
    echo "Files affected: $files"
    echo ""
    echo "To track progress, run this script periodically and watch the numbers decrease!"
}

# Main execution
main() {
    show_top_offenders
    analyze_by_component
    find_critical_unwraps
    generate_fix_suggestions
    track_progress
    
    echo -e "\n✅ Analysis complete!"
    echo "Next step: Start with the top offenders and work your way down."
    echo "Use 'PRODUCTION_HARDENING_SPRINT.md' for detailed guidance."
}

main