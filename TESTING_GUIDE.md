# 🧪 Blixard Enhanced Testing Guide

This guide shows you how to run all the Phase 3 enhanced testing infrastructure we built for the microvm.nix integration.

## 🚀 Quick Start

### Run All Tests
```bash
# Run all tests with output
cargo test --package blixard-vm -- --nocapture

# Run tests quietly (just results)
cargo test --package blixard-vm
```

### Run Core Distributed Tests
```bash
# Run core blixard tests with distributed features
cargo test --package blixard-core --features test-helpers

# Run simulation tests (MadSim deterministic testing)
cd simulation && cargo test
```

## 📊 Phase 3 Test Categories

### 1. **Performance Benchmarks** ⚡
Test VM operations performance and establish baselines.

```bash
# Run all performance tests
cargo test benchmark --package blixard-vm -- --nocapture

# Specific benchmarks
cargo test benchmark_vm_creation --package blixard-vm -- --nocapture
cargo test benchmark_concurrent_operations --package blixard-vm -- --nocapture
cargo test benchmark_flake_generation --package blixard-vm -- --nocapture
```

**Expected Performance:**
- VM Creation: **~150μs** (667x faster than 100ms target!)
- Flake Generation: **~14μs** (714x faster than 10ms target!)
- Concurrent Operations: 5 VMs in **~665μs**
- Memory Stability: 50+ VMs with no performance degradation

### 2. **VM Lifecycle Integration Tests** 🔄
Test complete VM lifecycle with real Nix builds.

```bash
# Run lifecycle tests
cargo test lifecycle --package blixard-vm -- --nocapture

# Specific lifecycle tests
cargo test test_complete_vm_lifecycle --package blixard-vm -- --nocapture
cargo test test_multiple_vm_lifecycle --package blixard-vm -- --nocapture
cargo test test_vm_config_persistence --package blixard-vm -- --nocapture
```

**What these test:**
- Create → Build → Verify → Delete cycles
- Multi-VM management scenarios
- Configuration persistence across restarts
- Real Nix flake building (when Nix available)

### 3. **Flake Validation Tests** ✅
Verify generated Nix flakes are correct and buildable.

```bash
# Run flake validation
cargo test validation --package blixard-vm -- --nocapture

# Specific validation tests
cargo test test_generated_flake_validation --package blixard-vm -- --nocapture
cargo test test_template_syntax --package blixard-vm -- --nocapture
```

**What these test:**
- `nix flake check` integration
- Template syntax correctness
- Variable substitution accuracy
- Complex configuration scenarios

### 4. **Error Scenario Tests** 🚨
Test error handling and edge cases.

```bash
# Run error scenario tests
cargo test error_scenario --package blixard-vm -- --nocapture

# Specific error tests
cargo test test_invalid_vm_configurations --package blixard-vm -- --nocapture
cargo test test_duplicate_vm_creation --package blixard-vm -- --nocapture
cargo test test_corrupted_config_recovery --package blixard-vm -- --nocapture
```

**What these test:**
- Invalid VM configurations
- Duplicate VM prevention
- Filesystem corruption recovery
- Resource exhaustion scenarios
- Concurrent operation conflicts

### 5. **Distributed VM Orchestration Tests** 🌐
Test VM management across multiple nodes (MadSim).

```bash
# Run distributed tests
cd simulation
cargo test vm_orchestration -- --nocapture

# Specific distributed tests
cargo test test_distributed_vm_placement -- --nocapture
cargo test test_vm_operations_during_partition -- --nocapture
cargo test test_node_failure_vm_recovery -- --nocapture
```

**What these test:**
- Multi-node VM placement
- Network partition tolerance
- Node failure recovery
- Distributed state consistency

## 🔧 Basic Component Tests

### Core VM Backend Tests
```bash
# Test basic VM backend functionality
cargo test backend_integration --package blixard-vm -- --nocapture

# Test Nix flake generation
cargo test nix_generation --package blixard-vm -- --nocapture
```

## 🎯 Targeted Test Execution

### Test Specific Components
```bash
# Test only flake generation
cargo test nix_generation_tests --package blixard-vm

# Test only error handling
cargo test error_scenario_tests --package blixard-vm

# Test only performance
cargo test performance_benchmarks --package blixard-vm
```

### Test with Different Verbosity
```bash
# Quiet mode (just pass/fail)
cargo test --package blixard-vm -q

# Verbose mode (show all output)
cargo test --package blixard-vm -- --nocapture

# Show only specific test output
cargo test benchmark_vm_creation --package blixard-vm -- --nocapture
```

## 🌍 Environment Considerations

### Running with Nix Available
If you have Nix installed, tests will perform actual flake building and validation:
```bash
# Full integration with real Nix builds
cargo test --package blixard-vm -- --nocapture
```

### Running without Nix (CI/Local)
Tests gracefully handle missing Nix and focus on code logic:
```bash
# Tests will skip Nix operations and validate syntax/logic
cargo test --package blixard-vm
```

### Running in CI Environments
```bash
# Automatic timeout scaling for slower CI environments
CI=true cargo test --package blixard-vm
```

## 📈 Interpreting Results

### Performance Benchmark Output
```
Created 10 VMs in 1.500794ms
Average creation time: 150.079µs
✓ Passes: < 100ms target (667x faster!)

Generated 100 flakes in 1.441042ms  
Average generation time: 14.41µs
✓ Passes: < 10ms target (714x faster!)
```

### Integration Test Output
```
✓ VM created successfully
✓ Flake generated at: vm-configs/vms/example-vm/flake.nix
⚠ Nix build skipped (no Nix available)
✓ VM deleted successfully
```

### Error Test Output
```
✓ Empty VM name correctly rejected
✓ Duplicate VM creation prevented  
✓ Corrupted config recovery successful
```

## 🔍 Debugging Failed Tests

### Common Issues and Solutions

**Compilation Errors:**
```bash
# Fix import warnings
cargo fix --package blixard-vm --allow-dirty

# Check specific error
cargo test test_name --package blixard-vm 2>&1 | head -50
```

**Permission Errors:**
```bash
# Ensure temp directories are writable
ls -la /tmp/

# Run with different temp location if needed
TMPDIR=/path/to/writable/dir cargo test --package blixard-vm
```

**Performance Test Failures:**
- Check system load: `htop` or `top`
- Run tests individually to isolate issues
- Adjust timeout expectations in test code

## 🚀 Advanced Usage

### Running Tests with Coverage
```bash
# Install cargo-tarpaulin if not available
cargo install cargo-tarpaulin

# Run with coverage
cargo tarpaulin --package blixard-vm --out Html --output-dir coverage/
```

### Running Tests with Different Features
```bash
# Run with all features
cargo test --package blixard-vm --all-features

# Run distributed tests specifically  
cargo test --package blixard-core --features test-helpers
```

### Stress Testing
```bash
# Run performance tests multiple times
for i in {1..5}; do
  echo "=== Run $i ==="
  cargo test benchmark --package blixard-vm -- --nocapture
done
```

## 📚 Test Organization

```
blixard-vm/tests/
├── backend_integration_test.rs     # Basic VM backend functionality
├── error_scenario_tests.rs         # Error handling and edge cases  
├── flake_validation_tests.rs       # Nix flake correctness
├── nix_generation_tests.rs         # Template generation
├── performance_benchmarks.rs       # Performance monitoring
└── vm_lifecycle_integration_tests.rs # Full lifecycle testing

simulation/tests/
└── vm_orchestration_tests.rs       # Distributed VM management
```

## 🎯 Next Steps

After running the enhanced test suite:

1. **Monitor Performance**: Use benchmark results to establish baselines
2. **Review Coverage**: Ensure all critical paths are tested  
3. **Iterate**: Add tests for new features as they're developed
4. **Integrate**: Include in CI/CD pipeline for continuous validation

The enhanced testing infrastructure ensures our microvm.nix integration is robust, performant, and production-ready! 🎉