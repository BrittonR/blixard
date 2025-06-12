#!/bin/bash
# Run simulation tests with madsim

set -e

echo "🧪 Running Blixard simulation tests..."
echo

# Set up environment
export RUST_LOG=${RUST_LOG:-info}
export MADSIM_TEST_SEED=${MADSIM_TEST_SEED:-12345}

echo "📝 Configuration:"
echo "   RUST_LOG: $RUST_LOG"
echo "   MADSIM_TEST_SEED: $MADSIM_TEST_SEED"
echo

# Build with madsim cfg
echo "🔨 Building simulation tests..."
# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/../simulation"
RUSTFLAGS="--cfg madsim" cargo build --release --tests

# Run the tests
echo "🚀 Running tests..."
RUSTFLAGS="--cfg madsim" cargo test --release -- --nocapture

echo
echo "✅ Simulation tests completed!"