#!/bin/bash

# Script to help identify and categorize dead code warnings

echo "=== Dead Code Analysis ==="
echo ""

echo "1. Unused fields that are never read:"
cargo check 2>&1 | grep -E "field.*is never read" | sort | uniq
echo ""

echo "2. Unused methods:"
cargo check 2>&1 | grep -E "method.*is never used" | sort | uniq
echo ""

echo "3. Unused functions:"
cargo check 2>&1 | grep -E "function.*is never used" | sort | uniq
echo ""

echo "4. Unused enum variants:"
cargo check 2>&1 | grep -E "variant.*never constructed" | sort | uniq
echo ""

echo "5. Unused structs:"
cargo check 2>&1 | grep -E "struct.*is never constructed" | sort | uniq
echo ""

echo "6. Unused variables:"
cargo check 2>&1 | grep -E "unused variable" | sort | uniq
echo ""

echo "7. Variables that don't need to be mutable:"
cargo check 2>&1 | grep -E "variable does not need to be mutable" | sort | uniq
echo ""

echo "Total warnings: $(cargo check 2>&1 | grep -E 'warning:' | wc -l)"