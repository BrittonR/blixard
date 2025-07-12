#!/bin/bash

# Script to systematically fix warnings by adding #[allow(dead_code)] annotations
# for unused fields and removing unused imports

echo "=== Fixing warnings systematically ==="

# Get list of unused field warnings
echo "Finding unused field warnings..."
cargo build --all-features 2>&1 | grep -E "field.*is never read" | head -20 > /tmp/unused_fields.txt

# Get list of unused import warnings  
echo "Finding unused import warnings..."
cargo build --all-features 2>&1 | grep -E "unused import" | head -20 > /tmp/unused_imports.txt

# Show what we found
echo "Found these unused field warnings:"
cat /tmp/unused_fields.txt | head -10

echo -e "\nFound these unused import warnings:"
cat /tmp/unused_imports.txt | head -10

echo -e "\nTotal warnings: $(cargo build --all-features 2>&1 | grep "warning:" | wc -l)"