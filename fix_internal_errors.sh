#!/bin/bash

# Script to fix missing source field in BlixardError::Internal initializers

echo "Fixing missing source field in BlixardError::Internal initializers..."

# Find all files with missing source field errors for Internal variant
files=$(cargo check -p blixard-core --message-format=short 2>&1 | \
        rg "missing field.*source.*Internal" | \
        cut -d: -f1 | sort | uniq)

for file in $files; do
    echo "Processing $file..."
    
    # Fix Internal errors missing source field
    # Pattern: BlixardError::Internal { message: ... }
    # Replace with: BlixardError::Internal { message: ..., source: None }
    sed -i 's/BlixardError::Internal {[^}]*message: \([^}]*\)}/BlixardError::Internal { message: \1, source: None }/g' "$file"
done

echo "Fixed Internal error initializers in files: $files"