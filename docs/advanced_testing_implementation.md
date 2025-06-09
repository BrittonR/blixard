# Advanced Testing Implementation Guide for Seaglass

This guide shows how to implement the advanced testing methodologies from `advanced_testing_methodologies.md` in the Seaglass project using existing Rust crates.

**Important**: This project uses `cargo-nextest` for test execution. Throughout this guide, use `cargo nextest run` (or the alias `cargo nt`) instead of `cargo test` for better performance, cleaner output, and proper test isolation.

## Table of Contents
1. [Quick Start](#quick-start)
2. [Deterministic Simulation Testing](#deterministic-simulation-testing)
3. [Property-Based Testing](#property-based-testing)
4. [Fault Injection Framework](#fault-injection-framework)
5. [Model Checking](#model-checking)
6. [Continuous Verification](#continuous-verification)
7. [Integration Examples](#integration-examples)

## Quick Start

Add these dependencies to your `Cargo.toml`:

```toml
[dependencies]
# Existing dependencies...
fail = { version = "0.5", features = ["failpoints"] }

[dev-dependencies]
# Deterministic simulation
madsim = "0.2"
turmoil = "0.6"
mock_instant = "0.3"

# Property testing
proptest = "1.4"
test-case = "3.3"
rstest = "0.18"

# Model checking
stateright = "0.30"

# Mutation testing (works with nextest)
cargo-mutants = "24.1"
```

## Deterministic Simulation Testing

### 1. Basic Setup with madsim

Create a simulation harness for pipeline execution:

```rust
// tests/simulation/mod.rs
use madsim::time::{Duration, Instant};
use madsim::rand::{self, Rng};
use std::sync::Arc;

pub struct SimulatedEnvironment {
    seed: u64,
    clock: Arc<SimulatedClock>,
    fs: Arc<SimulatedFileSystem>,
    rng: rand::rngs::StdRng,
}

impl SimulatedEnvironment {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            clock: Arc::new(SimulatedClock::new()),
            fs: Arc::new(SimulatedFileSystem::new()),
            rng: rand::rngs::StdRng::seed_from_u64(seed),
        }
    }
    
    pub fn tick(&mut self, duration: Duration) {
        self.clock.advance(duration);
    }
}

// Simulated file system for deterministic I/O
pub struct SimulatedFileSystem {
    files: std::collections::HashMap<PathBuf, Vec<u8>>,
    failures: Vec<(PathBuf, IoError)>,
}

impl SimulatedFileSystem {
    pub fn inject_failure(&mut self, path: PathBuf, error: IoError) {
        self.failures.push((path, error));
    }
}
```

### 2. Deterministic Pipeline Executor

Modify the pipeline executor to support simulation:

```rust
// src/pipeline/deterministic_executor.rs
use crate::pipeline::{Pipeline, ExecutorError};
use fail::fail_point;

pub trait ExecutionEnvironment: Send + Sync {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, io::Error>;
    fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), io::Error>;
    fn now(&self) -> Instant;
    fn random(&mut self) -> f64;
}

pub struct DeterministicExecutor<E: ExecutionEnvironment> {
    env: E,
}

impl<E: ExecutionEnvironment> DeterministicExecutor<E> {
    pub async fn execute(&mut self, pipeline: &mut Pipeline) -> Result<(), ExecutorError> {
        let start = self.env.now();
        
        for node in &mut pipeline.nodes {
            // Inject failures for testing
            fail_point!("node_execution", |_| {
                Err(ExecutorError::NodeFailed(node.id.clone()))
            });
            
            // Deterministic execution
            let data = self.load_node_data(&node).await?;
            let result = self.execute_transformations(&node, data).await?;
            self.save_node_output(&node, result).await?;
        }
        
        let elapsed = self.env.now() - start;
        log::info!("Pipeline executed in {:?}", elapsed);
        Ok(())
    }
}
```

### 3. Simulation Test Example

```rust
// tests/simulation_tests.rs
use madsim::{runtime::Runtime, time::Duration};

#[madsim::test]
async fn test_pipeline_with_failures() {
    let mut env = SimulatedEnvironment::new(42); // Deterministic seed
    let mut executor = DeterministicExecutor::new(env);
    
    // Create test pipeline
    let mut pipeline = create_test_pipeline();
    
    // Run with different failure scenarios
    for scenario in 0..100 {
        env.reset();
        
        // Inject random failures
        if scenario % 10 == 0 {
            env.fs.inject_failure(
                PathBuf::from("data/input.csv"),
                io::Error::new(io::ErrorKind::NotFound, "File not found")
            );
        }
        
        // Execute and verify
        let result = executor.execute(&mut pipeline).await;
        verify_pipeline_invariants(&pipeline, result);
    }
}

#[test]
fn test_deterministic_replay() {
    // Run with same seed should produce identical results
    let result1 = run_simulation_with_seed(12345);
    let result2 = run_simulation_with_seed(12345);
    assert_eq!(result1, result2);
}
```

## Property-Based Testing

### 1. DataFrame Property Tests

```rust
// tests/property_tests.rs
use proptest::prelude::*;
use polars::prelude::*;

// Custom strategy for generating DataFrames
prop_compose! {
    fn arbitrary_dataframe()(
        num_rows in 0..1000usize,
        columns in prop::collection::vec(
            (any::<String>(), any::<DataType>()),
            1..10
        )
    ) -> DataFrame {
        let mut df = DataFrame::empty();
        for (name, dtype) in columns {
            let series = match dtype {
                DataType::Int32 => Series::new(&name, 
                    (0..num_rows).map(|_| rand::random::<i32>()).collect::<Vec<_>>()
                ),
                DataType::Utf8 => Series::new(&name,
                    (0..num_rows).map(|_| random_string()).collect::<Vec<_>>()
                ),
                // Add more types...
            };
            df = df.with_column(series).unwrap();
        }
        df
    }
}

proptest! {
    #[test]
    fn test_transformation_preserves_row_count(
        input in arbitrary_dataframe(),
        transform in any::<Transformation>()
    ) {
        prop_assume!(!transform.is_filter()); // Filters can change row count
        
        let result = apply_transformation(&input, &transform);
        prop_assert_eq!(result.height(), input.height());
    }
    
    #[test]
    fn test_pipeline_deterministic(
        input in arbitrary_dataframe(),
        pipeline in arbitrary_pipeline()
    ) {
        let result1 = execute_pipeline(input.clone(), pipeline.clone());
        let result2 = execute_pipeline(input.clone(), pipeline.clone());
        prop_assert_eq!(result1, result2);
    }
    
    #[test]
    fn test_validation_completeness(
        data in arbitrary_dataframe(),
        validations in prop::collection::vec(any::<Validation>(), 0..10)
    ) {
        let errors = validate_data(&data, &validations);
        
        // Every error should reference a valid row
        for error in &errors {
            prop_assert!(error.row_index < data.height());
        }
        
        // No duplicate errors for same cell
        let mut seen = HashSet::new();
        for error in &errors {
            let key = (error.row_index, error.column.clone());
            prop_assert!(!seen.contains(&key));
            seen.insert(key);
        }
    }
}
```

### 2. Transformation Property Tests

```rust
// tests/transformation_properties.rs
use proptest::prelude::*;

// Generate arbitrary transformations
#[derive(Debug, Clone, Arbitrary)]
enum TestTransformation {
    Replace {
        #[proptest(strategy = "any::<String>()")]
        column: String,
        #[proptest(strategy = "any::<String>()")]
        from: String,
        #[proptest(strategy = "any::<String>()")]
        to: String,
    },
    Filter {
        #[proptest(strategy = "valid_where_clause()")]
        where_clause: String,
    },
    Concatenate {
        #[proptest(strategy = "prop::collection::vec(any::<String>(), 2..5)")]
        columns: Vec<String>,
        #[proptest(strategy = "any::<Option<String>>()")]
        separator: Option<String>,
    },
}

fn valid_where_clause() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("age > 18".to_string()),
        Just("status = 'active'".to_string()),
        Just("(age > 18 AND status = 'active')".to_string()),
    ]
}

proptest! {
    #[test]
    fn test_transformation_idempotence(
        data in arbitrary_dataframe(),
        transform in any::<TestTransformation>()
    ) {
        // Some transformations should be idempotent
        if transform.is_idempotent() {
            let once = apply_transformation(&data, &transform)?;
            let twice = apply_transformation(&once, &transform)?;
            prop_assert_eq!(once, twice);
        }
    }
}
```

## Fault Injection Framework

### 1. Setting Up fail-rs

```rust
// src/pipeline/executor.rs
use fail::fail_point;

impl PipelineExecutor {
    pub async fn execute_node(&mut self, node: &Node) -> Result<DataFrame> {
        // Inject I/O failures
        fail_point!("io_read_failure", |_| {
            Err(ExecutorError::IoError(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Simulated permission denied"
            )))
        });
        
        let data = self.load_data(&node.input)?;
        
        // Inject processing failures
        fail_point!("transform_failure", |msg| {
            if let Some(msg) = msg {
                if msg == "corrupt" {
                    // Simulate data corruption
                    return Ok(corrupt_dataframe(data));
                }
            }
            Err(ExecutorError::TransformFailed)
        });
        
        // Normal execution
        let result = self.apply_transformations(data, &node.transformations)?;
        
        // Inject write failures
        fail_point!("io_write_failure", |_| {
            Err(ExecutorError::IoError(io::Error::new(
                io::ErrorKind::NoSpace,
                "Simulated disk full"
            )))
        });
        
        self.save_data(&node.output, result)?;
        Ok(result)
    }
}
```

### 2. Fault Injection Tests

```rust
// tests/fault_injection_tests.rs
use fail::{FailScenario, cfg};

#[test]
fn test_pipeline_handles_io_failures() {
    let _scenario = FailScenario::setup();
    
    // Configure 50% failure rate
    cfg("io_read_failure", "50%return").unwrap();
    
    let mut successes = 0;
    let mut failures = 0;
    
    for _ in 0..100 {
        match execute_pipeline() {
            Ok(_) => successes += 1,
            Err(ExecutorError::IoError(_)) => failures += 1,
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
    
    // Should be roughly 50/50
    assert!(successes > 40 && successes < 60);
}

#[test]
fn test_data_corruption_detection() {
    let _scenario = FailScenario::setup();
    
    // Inject corruption
    cfg("transform_failure", "return(corrupt)").unwrap();
    
    let result = execute_pipeline();
    assert!(matches!(result, Err(ExecutorError::DataCorruption)));
}

#[rstest]
#[case::network_partition("network_failure", "return")]
#[case::disk_full("io_write_failure", "return")]
#[case::memory_pressure("allocation_failure", "return")]
fn test_failure_scenarios(#[case] fail_point: &str, #[case] config: &str) {
    let _scenario = FailScenario::setup();
    cfg(fail_point, config).unwrap();
    
    let pipeline = create_test_pipeline();
    let result = execute_pipeline(pipeline);
    
    // Verify graceful handling
    assert!(result.is_err());
    // Verify no data corruption
    verify_no_partial_writes();
}
```

### 3. Chaos Testing

```rust
// tests/chaos_tests.rs
use rand::Rng;

struct ChaosMonkey {
    failure_rate: f64,
    rng: rand::rngs::StdRng,
}

impl ChaosMonkey {
    fn inject_failures(&mut self) {
        if self.rng.gen::<f64>() < self.failure_rate {
            match self.rng.gen_range(0..5) {
                0 => cfg("io_read_failure", "return").unwrap(),
                1 => cfg("io_write_failure", "return").unwrap(),
                2 => cfg("transform_failure", "return").unwrap(),
                3 => cfg("memory_allocation", "return").unwrap(),
                4 => cfg("network_timeout", "sleep(5000)").unwrap(),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn chaos_test_pipeline_resilience() {
    let mut monkey = ChaosMonkey {
        failure_rate: 0.1,
        rng: rand::rngs::StdRng::seed_from_u64(42),
    };
    
    for i in 0..1000 {
        let _scenario = FailScenario::setup();
        monkey.inject_failures();
        
        let pipeline = generate_random_pipeline(i);
        match execute_pipeline(pipeline) {
            Ok(result) => verify_result_correctness(result),
            Err(e) => verify_error_handling(e),
        }
    }
}
```

## Model Checking

### 1. Stateright Model for Pipeline

```rust
// tests/model_checking.rs
use stateright::*;

#[derive(Clone, Debug, Hash)]
struct PipelineState {
    nodes_completed: Vec<bool>,
    data_versions: HashMap<String, u64>,
    errors: Vec<String>,
}

#[derive(Clone, Debug, Hash)]
enum PipelineAction {
    ExecuteNode(usize),
    InjectFailure(usize),
    RestartNode(usize),
}

struct PipelineModel {
    num_nodes: usize,
}

impl Model for PipelineModel {
    type State = PipelineState;
    type Action = PipelineAction;
    
    fn init_states(&self) -> Vec<Self::State> {
        vec![PipelineState {
            nodes_completed: vec![false; self.num_nodes],
            data_versions: HashMap::new(),
            errors: Vec::new(),
        }]
    }
    
    fn actions(&self, state: &Self::State, _: &mut Vec<Self::Action>) -> Vec<Self::Action> {
        let mut actions = Vec::new();
        
        for i in 0..self.num_nodes {
            if !state.nodes_completed[i] {
                actions.push(PipelineAction::ExecuteNode(i));
                actions.push(PipelineAction::InjectFailure(i));
            } else if state.errors.iter().any(|e| e.contains(&format!("node_{}", i))) {
                actions.push(PipelineAction::RestartNode(i));
            }
        }
        
        actions
    }
    
    fn next(&self, state: &Self::State, action: Self::Action) -> Self::State {
        let mut next = state.clone();
        
        match action {
            PipelineAction::ExecuteNode(i) => {
                next.nodes_completed[i] = true;
                let version = state.data_versions.get(&format!("node_{}", i))
                    .copied()
                    .unwrap_or(0) + 1;
                next.data_versions.insert(format!("node_{}", i), version);
            }
            PipelineAction::InjectFailure(i) => {
                next.errors.push(format!("Failed at node_{}", i));
            }
            PipelineAction::RestartNode(i) => {
                next.nodes_completed[i] = false;
                next.errors.retain(|e| !e.contains(&format!("node_{}", i)));
            }
        }
        
        next
    }
}

#[test]
fn model_check_pipeline() {
    let model = PipelineModel { num_nodes: 3 };
    
    // Check that pipeline eventually completes
    model.checker()
        .spawn_dfs()
        .join()
        .assert_eventually(|_, state| {
            state.nodes_completed.iter().all(|&completed| completed)
        });
    
    // Check no data races
    model.checker()
        .spawn_bfs()
        .join()
        .assert_always(|_, state| {
            // Each node output has monotonic version numbers
            state.data_versions.values().all(|&v| v > 0)
        });
}
```

### 2. Linearizability Testing

```rust
// tests/linearizability_tests.rs
use stateright::semantics::*;

#[derive(Clone, Debug)]
struct CacheOp {
    key: String,
    value: Option<String>,
    op_type: OpType,
}

#[derive(Clone, Debug)]
enum OpType {
    Read,
    Write(String),
    Delete,
}

fn test_cache_linearizability() {
    let mut history = Vec::new();
    
    // Record operations from concurrent execution
    let cache = Arc::new(Mutex::new(UnifiedCache::new()));
    let mut handles = vec![];
    
    for thread_id in 0..4 {
        let cache_clone = cache.clone();
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let op = match rand::random::<u8>() % 3 {
                    0 => CacheOp {
                        key: format!("key_{}", i % 10),
                        value: None,
                        op_type: OpType::Read,
                    },
                    1 => CacheOp {
                        key: format!("key_{}", i % 10),
                        value: Some(format!("value_{}_{}", thread_id, i)),
                        op_type: OpType::Write(format!("value_{}_{}", thread_id, i)),
                    },
                    _ => CacheOp {
                        key: format!("key_{}", i % 10),
                        value: None,
                        op_type: OpType::Delete,
                    },
                };
                
                // Execute and record
                let start = Instant::now();
                let result = execute_cache_op(&cache_clone, &op);
                let end = Instant::now();
                
                history.push((start, end, op, result));
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Check linearizability
    assert!(is_linearizable(&history));
}
```

## Continuous Verification

### 1. Long-Running Property Tests

```rust
// tests/continuous/mod.rs
use std::time::Duration;

pub struct ContinuousTestRunner {
    test_duration: Duration,
    seed_generator: rand::rngs::StdRng,
}

impl ContinuousTestRunner {
    pub fn run_continuous_tests(&mut self) {
        let start = Instant::now();
        let mut iterations = 0;
        let mut bugs_found = Vec::new();
        
        while start.elapsed() < self.test_duration {
            let seed = self.seed_generator.gen();
            
            match self.run_single_test(seed) {
                Ok(()) => iterations += 1,
                Err(bug) => {
                    bugs_found.push((seed, bug));
                    self.minimize_bug(seed);
                }
            }
            
            if iterations % 1000 == 0 {
                log::info!("Completed {} iterations, found {} bugs", 
                    iterations, bugs_found.len());
            }
        }
    }
    
    fn minimize_bug(&self, seed: u64) {
        // Binary search to find minimal reproduction
        let mut test_case = self.generate_test_case(seed);
        
        while test_case.can_shrink() {
            let smaller = test_case.shrink();
            if self.reproduces_bug(&smaller) {
                test_case = smaller;
            }
        }
        
        log::error!("Minimal bug reproduction: {:?}", test_case);
    }
}
```

### 2. Integration with CI

```yaml
# .github/workflows/continuous-testing.yml
name: Continuous Property Testing

on:
  schedule:
    - cron: '0 */6 * * *'  # Every 6 hours
  workflow_dispatch:

jobs:
  continuous-test:
    runs-on: ubuntu-latest
    timeout-minutes: 360  # 6 hours
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      
      - name: Run continuous tests
        run: |
          cargo test --test continuous -- --test-duration 6h
        env:
          RUST_LOG: info
          
      - name: Upload bug reports
        if: failure()
        uses: actions/upload-artifact@v3
        with:
          name: bug-reports
          path: target/bug-reports/
```

## Integration Examples

### 1. Complete Test Suite Structure

```rust
// tests/lib.rs
mod simulation;
mod property_tests;
mod fault_injection;
mod model_checking;
mod continuous;

// Shared test utilities
pub mod common {
    use seaglass::pipeline::*;
    
    pub fn create_test_pipeline() -> Pipeline {
        // Standard test pipeline
    }
    
    pub fn verify_invariants(pipeline: &Pipeline) {
        // Common invariant checks
    }
}
```

### 2. Running Advanced Tests

**Note: This project uses cargo-nextest for test execution. Use `cargo nextest run` (or the alias `cargo nt`) instead of `cargo test` for better performance and output.**

```bash
# Run all property tests with nextest
cargo nextest run --test property_tests
# Or use the alias
cargo nt --test property_tests

# Run simulation tests with logging
RUST_LOG=debug cargo nextest run --test simulation_tests

# Run fault injection tests
FAILPOINTS=io_read_failure=return cargo nextest run --test fault_injection

# Run model checking (use release mode for performance)
cargo nextest run --test model_checking --release

# Run continuous tests for 1 hour
cargo nextest run --test continuous -- --test-duration 1h

# Run all tests with all features
cargo nextest run --all-features
# Or use the alias
cargo nta

# Run mutation testing with nextest
cargo mutants --test-tool nextest
```

### 3. Example: End-to-End Deterministic Test

```rust
// tests/e2e_deterministic.rs
#[madsim::test]
async fn test_full_pipeline_deterministic() {
    let seed = 12345;
    let mut sim = SimulatedEnvironment::new(seed);
    
    // Create complex pipeline
    let pipeline = Pipeline {
        nodes: vec![
            Node {
                id: "load".to_string(),
                transformations: vec![],
                input: Input::File("data.csv".into()),
                output: Output::Memory,
            },
            Node {
                id: "clean".to_string(),
                transformations: vec![
                    Transformation::Trim { column: "name".into() },
                    Transformation::Filter { 
                        where_clause: "age > 0 AND age < 150".into() 
                    },
                ],
                dependencies: vec!["load".to_string()],
            },
            Node {
                id: "transform".to_string(),
                transformations: vec![
                    Transformation::Concatenate {
                        columns: vec!["first_name".into(), "last_name".into()],
                        output_column: "full_name".into(),
                        separator: Some(" ".into()),
                    },
                ],
                dependencies: vec!["clean".to_string()],
            },
        ],
    };
    
    // Inject some failures
    sim.fs.set_failure_rate(0.1);  // 10% I/O failure rate
    
    // Execute with retries
    let mut executor = DeterministicExecutor::new(sim);
    let result = executor.execute_with_retries(&pipeline, 3).await;
    
    // Verify determinism
    let mut sim2 = SimulatedEnvironment::new(seed);
    sim2.fs.set_failure_rate(0.1);
    let mut executor2 = DeterministicExecutor::new(sim2);
    let result2 = executor2.execute_with_retries(&pipeline, 3).await;
    
    assert_eq!(result, result2);  // Same seed = same result
}
```

## Best Practices

1. **Start Small**: Begin with property tests for core transformations
2. **Add Determinism Gradually**: Wrap I/O and time operations first
3. **Focus on Invariants**: Define clear properties that must always hold
4. **Use cargo-nextest**: Already configured in the project - use `cargo nt` for faster test execution
5. **Automate Bug Minimization**: Make debugging easier
6. **Track Coverage**: Use tarpaulin or llvm-cov for coverage metrics
7. **Document Found Bugs**: Build a regression test suite
8. **Parallelize Tests**: Nextest runs tests in parallel by default for better performance

## Next Steps

1. **Phase 1**: Add proptest for transformation testing
2. **Phase 2**: Implement fail-rs for fault injection
3. **Phase 3**: Create deterministic test harness with madsim
4. **Phase 4**: Add stateright for model checking critical paths
5. **Phase 5**: Set up continuous verification in CI

This implementation plan provides a practical path to achieving TigerBeetle/FoundationDB-level testing reliability in Seaglass.