#!/bin/bash
set -e

echo "=== Running Blixard Test Suite ==="

echo -e "\n1. Running unit tests..."
cargo test --lib

echo -e "\n2. Running CLI tests..."
cargo test --test cli_tests

echo -e "\n3. Running error handling tests..."
cargo test --test error_tests

echo -e "\n4. Running node tests..."
cargo test --test node_tests

echo -e "\n5. Running type tests..."
cargo test --test types_tests

echo -e "\n6. Running storage tests..."
cargo test --test storage_tests

echo -e "\n7. Running property-based tests..."
cargo test proptest

echo -e "\n8. Running stateright model checking..."
cargo test --test stateright_simple_test

echo -e "\n9. Building with all features..."
cargo build --all-features

echo -e "\n10. Running clippy..."
cargo clippy -- -D warnings

echo -e "\n=== All tests passed! ==="