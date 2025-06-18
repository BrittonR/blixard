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

# Build with madsim cfg (handled by .cargo/config.toml)
echo "🔨 Building simulation tests..."
# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/../simulation"
cargo build --release --tests

# Check if cargo-nextest is installed
if command -v cargo-nextest &> /dev/null; then
    echo "🚀 Running tests with nextest..."
    cargo nextest run --release --no-capture
else
    echo "🚀 Running tests..."
    cargo test --release -- --nocapture
fi

echo
echo "✅ Simulation tests completed!"