#!/bin/bash
# Quick test script for current Rust implementation

set -e

echo "=== Testing Current Blixard Rust Implementation ==="
echo

# 1. Run all unit tests
echo "1. Running unit tests..."
cargo test --lib --no-fail-fast 2>&1 | grep -E "(test .* \.\.\.|test result:)" || true

echo -e "\n2. Running property tests..."
cargo test property_test 2>&1 | grep -E "(test .* \.\.\.|test result:)" || true

echo -e "\n3. Running integration tests (single-threaded)..."
cargo test --test "*" -- --test-threads=1 --nocapture 2>&1 | grep -E "(test .* \.\.\.|test result:|FAILED)" || true

echo -e "\n4. Running simulation tests with madsim..."
cargo test --features simulation simulation 2>&1 | grep -E "(test .* \.\.\.|test result:)" || true

echo -e "\n5. Testing CLI..."
cargo run -- --help 2>&1 | head -20 || true

echo -e "\n6. Building release binary..."
cargo build --release
ls -la target/release/blixard

echo -e "\nDone! Check output above for any failures."