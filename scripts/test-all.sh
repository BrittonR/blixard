#!/bin/bash
set -e

echo "=== Running Blixard Test Suite ==="

# Check if cargo-nextest is installed
if ! command -v cargo-nextest &> /dev/null; then
    echo "cargo-nextest is not installed. Install it with:"
    echo "  cargo install cargo-nextest --locked"
    echo ""
    echo "Falling back to cargo test..."
    USE_NEXTEST=false
else
    USE_NEXTEST=true
    echo "Using cargo nextest for improved test execution"
fi

if [ "$USE_NEXTEST" = true ]; then
    echo -e "\n1. Running all tests with nextest..."
    cargo nextest run
    
    echo -e "\n2. Running doctests (nextest doesn't support these)..."
    cargo test --doc
else
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
fi

echo -e "\n9. Building with all features..."
cargo build --all-features

echo -e "\n10. Running clippy..."
cargo clippy -- -D warnings

echo -e "\n=== All tests passed! ==="