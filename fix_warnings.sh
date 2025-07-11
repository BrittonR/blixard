#!/bin/bash

# Script to systematically fix Rust warnings
echo "=== Rust Warning Cleanup Script ==="

# Get warnings and process them
cargo build --all-features 2>&1 | grep -A 2 "warning: unused variable:" | while read -r line; do
    if [[ $line =~ "warning: unused variable:" ]]; then
        # Extract variable name
        var_name=$(echo "$line" | sed -n "s/.*unused variable: \`\(.*\)\`.*/\1/p")
        echo "Found unused variable: $var_name"
    elif [[ $line =~ "-->" ]]; then
        # Extract file and line number
        file_info=$(echo "$line" | awk '{print $2}')
        file_path=$(echo "$file_info" | cut -d: -f1)
        line_num=$(echo "$file_info" | cut -d: -f2)
        echo "  In file: $file_path at line $line_num"
    fi
done