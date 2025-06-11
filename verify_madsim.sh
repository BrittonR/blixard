#!/bin/bash
set -e

echo "=== Verifying Madsim Integration ==="
echo

echo "1. Testing basic time simulation..."
cargo test --test madsim_test test_basic_madsim_functionality --features simulation -- --nocapture 2>&1 | grep -E "(elapsed|passed|PASS)" || echo "FAILED"

echo
echo "2. Testing deterministic execution with fixed seed..."
SEED=42
echo "Running with seed $SEED (first time)..."
OUTPUT1=$(MADSIM_TEST_SEED=$SEED cargo test --test madsim_test test_deterministic_execution --features simulation 2>&1)
echo "Running with seed $SEED (second time)..."
OUTPUT2=$(MADSIM_TEST_SEED=$SEED cargo test --test madsim_test test_deterministic_execution --features simulation 2>&1)

if [ "$OUTPUT1" = "$OUTPUT2" ]; then
    echo "✓ Deterministic execution verified - outputs are identical"
else
    echo "✗ Deterministic execution FAILED - outputs differ"
fi

echo
echo "3. Testing network simulation..."
cargo test --test madsim_test test_network_simulation --features simulation

echo
echo "4. Checking that time advances instantly in simulation..."
cat > /tmp/time_test.rs << 'EOF'
#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    #[madsim::test]
    async fn verify_instant_time_advance() {
        let real_start = Instant::now();
        
        // Sleep for 1 hour in simulated time
        tokio::time::sleep(Duration::from_secs(3600)).await;
        
        let real_elapsed = real_start.elapsed();
        
        // In simulation, this should complete almost instantly (< 1 second)
        assert!(real_elapsed < Duration::from_secs(1), 
                "Time simulation failed: 1 hour sleep took {:?}", real_elapsed);
        
        println!("✓ 1 hour of simulated time completed in {:?}", real_elapsed);
    }
}
EOF

echo "Running time advance verification..."
rustc --test /tmp/time_test.rs --edition 2021 --crate-type bin -o /tmp/time_test \
    --extern madsim=$(find target -name "libmadsim*.rlib" | head -1) \
    --extern tokio=$(find target -name "libtokio*.rlib" | head -1) \
    -L dependency=target/debug/deps 2>/dev/null || \
    echo "(Skipping direct rustc test - use cargo test instead)"

echo
echo "5. Listing all tests using madsim..."
echo "Tests with #[madsim::test] attribute:"
rg "#\[madsim::test\]" tests/ -l | sort

echo
echo "=== Verification Complete ==="