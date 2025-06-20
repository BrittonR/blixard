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
    "all")
        echo "Running all MadSim tests..."
        cargo nextest run --test byzantine_tests --test clock_skew_tests
        ;;
    *)
        echo "Usage: $0 [all|byzantine|clock-skew]"
        exit 1
        ;;
esac

echo "✅ MadSim tests completed successfully!"