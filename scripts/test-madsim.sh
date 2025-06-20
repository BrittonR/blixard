#!/bin/bash
# Script to run MadSim tests with proper environment setup

set -e

echo "Running MadSim tests..."

cd "$(dirname "$0")/../simulation"

export RUSTFLAGS="--cfg madsim"

case "${1:-all}" in
    "byzantine")
        echo "Running Byzantine failure tests..."
        cargo nextest run --test byzantine_tests
        ;;
    "clock-skew")
        echo "Running clock skew tests..."
        cargo nextest run --test clock_skew_tests
        ;;
    "special")
        echo "Running special MadSim tests (Byzantine + clock skew)..."
        cargo nextest run --test byzantine_tests --test clock_skew_tests
        ;;
    "all")
        echo "Running ALL simulation tests..."
        cargo nextest run
        ;;
    *)
        echo "Usage: $0 [all|byzantine|clock-skew|special]"
        echo "  all        - Run all simulation tests (including moved distributed tests)"
        echo "  byzantine  - Run only Byzantine failure tests"
        echo "  clock-skew - Run only clock skew tests"
        echo "  special    - Run only Byzantine + clock skew tests"
        exit 1
        ;;
esac

echo "✅ MadSim tests completed successfully!"