//! VOPR (Viewstamped Operation Replication) Fuzzer for Blixard
//!
//! A TigerBeetle-inspired fuzzer for testing distributed consensus systems
//! with time acceleration, deterministic execution, and comprehensive state tracking.

pub mod fuzzer_engine;
pub mod invariants;
pub mod operation_generator;
pub mod shrink;
pub mod state_tracker;
pub mod test_harness;
pub mod time_accelerator;
pub mod visualizer;

pub use self::fuzzer_engine::{FuzzConfig, FuzzMode, FuzzerEngine};
pub use self::invariants::{Invariant, InvariantChecker, LivenessInvariant, SafetyInvariant};
pub use self::operation_generator::{Operation, OperationGenerator};
pub use self::state_tracker::{StateSnapshot, StateTracker};
pub use self::test_harness::{ClusterState, TestHarness};
pub use self::time_accelerator::{SimulatedTime, TimeAccelerator};

use std::sync::Arc;

/// Configuration for the VOPR fuzzer
#[derive(Debug, Clone)]
pub struct VoprConfig {
    /// Random seed for reproducibility
    pub seed: u64,

    /// Time acceleration factor (e.g., 1000 = 1000x faster than real time)
    pub time_acceleration: u64,

    /// Maximum clock skew between nodes in milliseconds
    pub max_clock_skew_ms: u64,

    /// Whether to enable liveness checks
    pub check_liveness: bool,

    /// Whether to enable safety checks
    pub check_safety: bool,

    /// Maximum number of operations to generate
    pub max_operations: usize,

    /// Enable shrinking/minimization of failing test cases
    pub enable_shrinking: bool,

    /// Output visualization data
    pub enable_visualization: bool,

    /// Coverage-guided fuzzing
    pub coverage_guided: bool,
}

impl Default for VoprConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            time_acceleration: 1000,
            max_clock_skew_ms: 5000,
            check_liveness: true,
            check_safety: true,
            max_operations: 10000,
            enable_shrinking: true,
            enable_visualization: true,
            coverage_guided: true,
        }
    }
}

/// Main VOPR fuzzer entry point
pub struct Vopr {
    config: VoprConfig,
    engine: FuzzerEngine,
    time: Arc<TimeAccelerator>,
    tracker: StateTracker,
    harness: TestHarness,
    invariant_checker: InvariantChecker,
}

impl Vopr {
    /// Create a new VOPR fuzzer with the given configuration
    pub fn new(config: VoprConfig) -> Self {
        let mut engine = FuzzerEngine::new(config.seed, config.coverage_guided);
        engine.set_seed(config.seed);

        let mut time = TimeAccelerator::new(config.time_acceleration, config.max_clock_skew_ms);
        time.set_seed(config.seed);

        let time = Arc::new(time);
        let tracker = StateTracker::new();
        let harness = TestHarness::new(Arc::clone(&time));
        let invariant_checker = InvariantChecker::new();

        Self {
            config,
            engine,
            time,
            tracker,
            harness,
            invariant_checker,
        }
    }

    /// Run the fuzzer
    pub async fn run(&mut self) -> Result<(), FuzzFailure> {
        println!("Starting VOPR fuzzer with seed {}", self.config.seed);

        let mut operations_executed = 0;

        while operations_executed < self.config.max_operations {
            // Generate test case
            let fuzz_config = FuzzConfig {
                max_operations: self.config.max_operations - operations_executed,
                operation_weights: Default::default(),
                coverage_guided: self.config.coverage_guided,
                min_nodes: 3,
                max_nodes: 5,
                fault_probability: 0.1,
                max_message_delay_ms: 1000,
            };

            let operations = self.engine.generate_test_case(&fuzz_config)
                .map_err(|e| FuzzFailure {
                    seed: self.config.seed,
                    operations: vec![],
                    violated_invariant: format!("Failed to generate test case: {}", e),
                    minimal_reproducer: None,
                    state_history: vec![],
                })?;
            let start_time = std::time::Instant::now();

            // Execute operations
            for (i, operation) in operations.iter().enumerate() {
                // Execute the operation
                if let Err(e) = self.harness.execute_operation(operation).await {
                    eprintln!("Error executing operation {}: {:?}", i, e);
                    continue;
                }

                // Advance time
                self.time.advance(std::time::Duration::from_millis(10));

                // Allow system to process
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;

                // Take state snapshot
                let snapshot = self.harness.capture_state_snapshot().await;
                self.tracker.snapshot(snapshot.clone());

                // Check invariants
                let violations = self.invariant_checker.check(&snapshot);
                if !violations.is_empty() {
                    // Found a bug!
                    println!("\n❌ INVARIANT VIOLATION DETECTED!");
                    for violation in &violations {
                        println!("  - {}", violation);
                    }

                    // Shrink the test case
                    let minimal = if self.config.enable_shrinking {
                        println!("\nShrinking test case...");
                        let test_fn = self.create_test_function(violations[0].clone());
                        let shrinker = shrink::Shrinker::new();
                        shrinker.shrink(operations.clone(), test_fn)
                    } else {
                        operations.clone()
                    };

                    // Generate failure report
                    return Err(FuzzFailure {
                        seed: self.config.seed,
                        operations: operations.clone(),
                        violated_invariant: violations[0].clone(),
                        minimal_reproducer: Some(minimal),
                        state_history: self.tracker.get_history(),
                    });
                }

                operations_executed += 1;

                // Print progress
                if operations_executed % 100 == 0 {
                    println!("Progress: {} operations executed", operations_executed);
                }
            }

            // Record execution for coverage
            let execution_time = start_time.elapsed();
            let coverage = self.harness.get_coverage();
            self.engine.record_execution(
                self.config.seed,
                operations,
                coverage,
                false,
                execution_time,
            );
        }

        println!("\n✅ Fuzzing completed successfully!");
        println!("Total operations executed: {}", operations_executed);

        // Print corpus statistics
        let stats = self.engine.corpus_stats();
        println!("\nCorpus Statistics:");
        println!("  Test cases: {}", stats.total_test_cases);
        println!("  Basic blocks covered: {}", stats.total_basic_blocks);
        println!("  Edges covered: {}", stats.total_edges);
        println!("  States explored: {}", stats.total_states);
        println!("  Average execution time: {:?}", stats.avg_execution_time);

        Ok(())
    }

    /// Create a test function for shrinking
    fn create_test_function(&self, _invariant: String) -> shrink::TestFunction {
        Box::new(move |ops| {
            // This is a simplified version - in reality, we'd re-run the operations
            // and check if the same invariant is violated
            !ops.is_empty()
        })
    }

    /// Generate a failure report
    pub fn generate_report(&self, failure: &FuzzFailure) -> String {
        let mut output = Vec::new();
        let visualizer = visualizer::Visualizer::new(Default::default());

        visualizer
            .generate_report(
                &mut output,
                failure.seed,
                &failure.operations,
                &failure.state_history,
                &[],
                Some(&failure.violated_invariant),
            )
            .unwrap();

        String::from_utf8(output).unwrap()
    }

    /// Generate a minimal reproducer
    pub fn generate_reproducer(&self, failure: &FuzzFailure) -> String {
        let mut output = Vec::new();
        let visualizer = visualizer::Visualizer::new(Default::default());

        let ops = failure
            .minimal_reproducer
            .as_ref()
            .unwrap_or(&failure.operations);

        visualizer
            .generate_reproducer(&mut output, failure.seed, ops)
            .unwrap();

        String::from_utf8(output).unwrap()
    }
}

/// Represents a fuzzing failure with details for reproduction
#[derive(Debug)]
pub struct FuzzFailure {
    /// The seed that caused the failure
    pub seed: u64,

    /// The sequence of operations that led to the failure
    pub operations: Vec<Operation>,

    /// The invariant that was violated
    pub violated_invariant: String,

    /// Minimal reproducer after shrinking
    pub minimal_reproducer: Option<Vec<Operation>>,

    /// State snapshots leading to the failure
    pub state_history: Vec<StateSnapshot>,
}
