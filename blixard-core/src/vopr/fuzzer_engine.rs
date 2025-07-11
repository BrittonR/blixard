//! Core fuzzing engine with coverage-guided operation generation
//!
//! This module implements the main fuzzing logic, including random operation
//! generation, coverage tracking, and execution orchestration.

use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::vopr::operation_generator::{Operation, OperationGenerator};
use crate::{acquire_lock, error::{BlixardError, BlixardResult}};

/// Fuzzing mode - safety vs liveness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzMode {
    /// Check safety properties (nothing bad happens)
    Safety,
    /// Check liveness properties (something good eventually happens)
    Liveness,
    /// Check both safety and liveness
    Both,
}

/// Configuration for the fuzzer engine
#[derive(Debug, Clone)]
pub struct FuzzConfig {
    /// Maximum operations per test
    pub max_operations: usize,

    /// Probability of each operation type
    pub operation_weights: OperationWeights,

    /// Enable coverage-guided fuzzing
    pub coverage_guided: bool,

    /// Minimum cluster size
    pub min_nodes: usize,

    /// Maximum cluster size
    pub max_nodes: usize,

    /// Probability of introducing faults
    pub fault_probability: f64,

    /// Maximum message delay in milliseconds
    pub max_message_delay_ms: u64,
}

/// Weights for different operation types
#[derive(Debug, Clone)]
pub struct OperationWeights {
    pub start_node: f64,
    pub stop_node: f64,
    pub restart_node: f64,
    pub client_request: f64,
    pub network_partition: f64,
    pub network_heal: f64,
    pub clock_jump: f64,
    pub clock_drift: f64,
    pub message_drop: f64,
    pub message_duplicate: f64,
    pub message_reorder: f64,
    pub byzantine_behavior: f64,
}

impl Default for OperationWeights {
    fn default() -> Self {
        Self {
            start_node: 0.1,
            stop_node: 0.1,
            restart_node: 0.05,
            client_request: 0.3,
            network_partition: 0.05,
            network_heal: 0.05,
            clock_jump: 0.02,
            clock_drift: 0.03,
            message_drop: 0.1,
            message_duplicate: 0.05,
            message_reorder: 0.05,
            byzantine_behavior: 0.05,
        }
    }
}

/// Coverage information for guided fuzzing
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    /// Set of basic blocks hit
    pub basic_blocks: HashSet<u64>,

    /// Edge coverage (from_bb -> to_bb)
    pub edges: HashSet<(u64, u64)>,

    /// State space coverage
    pub states_seen: HashSet<u64>,

    /// Interesting values seen
    pub interesting_values: HashMap<String, HashSet<u64>>,
}

impl Coverage {
    /// Check if this execution discovered new coverage
    fn has_new_coverage(&self, other: &Coverage) -> bool {
        // New basic blocks?
        if !self.basic_blocks.is_superset(&other.basic_blocks) {
            return true;
        }

        // New edges?
        if !self.edges.is_superset(&other.edges) {
            return true;
        }

        // New states?
        if !self.states_seen.is_superset(&other.states_seen) {
            return true;
        }

        false
    }

    /// Merge coverage from another execution
    fn merge(&mut self, other: &Coverage) {
        self.basic_blocks.extend(&other.basic_blocks);
        self.edges.extend(&other.edges);
        self.states_seen.extend(&other.states_seen);

        for (key, values) in &other.interesting_values {
            self.interesting_values
                .entry(key.clone())
                .or_insert_with(HashSet::new)
                .extend(values);
        }
    }
}

/// Main fuzzing engine
pub struct FuzzerEngine {
    /// Random number generator
    rng: ChaCha8Rng,

    /// Current seed
    seed: u64,

    /// Coverage-guided fuzzing enabled
    coverage_guided: bool,

    /// Global coverage information
    global_coverage: Arc<Mutex<Coverage>>,

    /// Corpus of interesting test cases
    corpus: Arc<Mutex<Vec<TestCase>>>,

    /// Operation generator
    op_generator: OperationGenerator,
}

/// A test case in the corpus
#[derive(Debug, Clone)]
struct TestCase {
    /// Seed that generated this test case
    seed: u64,

    /// Operations in this test case
    operations: Vec<Operation>,

    /// Coverage achieved by this test case
    coverage: Coverage,

    /// Whether this test case found a bug
    found_bug: bool,

    /// Execution time
    execution_time: std::time::Duration,
}

impl FuzzerEngine {
    /// Create a new fuzzer engine
    pub fn new(seed: u64, coverage_guided: bool) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            seed,
            coverage_guided,
            global_coverage: Arc::new(Mutex::new(Coverage::default())),
            corpus: Arc::new(Mutex::new(Vec::new())),
            op_generator: OperationGenerator::new(seed),
        }
    }

    /// Generate a random test case
    pub fn generate_test_case(&mut self, config: &FuzzConfig) -> BlixardResult<Vec<Operation>> {
        if self.coverage_guided && !acquire_lock!(self.corpus.lock(), "check corpus empty").is_empty() {
            // Sometimes mutate an existing test case from the corpus
            if self.rng.gen_bool(0.5) {
                return Ok(self.mutate_test_case(config));
            }
        }

        // Generate a fresh test case
        Ok(self.generate_random_operations(config))
    }

    /// Generate random operations
    fn generate_random_operations(&mut self, config: &FuzzConfig) -> Vec<Operation> {
        let num_operations = self.rng.gen_range(1..=config.max_operations);
        let mut operations = Vec::with_capacity(num_operations);

        // Start with some nodes
        let initial_nodes = self.rng.gen_range(config.min_nodes..=config.max_nodes);
        for node_id in 1..=initial_nodes {
            operations.push(Operation::StartNode {
                node_id: node_id as u64,
            });
        }

        // Generate random operations
        for _ in 0..num_operations {
            let op = self
                .op_generator
                .generate_weighted(&config.operation_weights, &mut self.rng);
            operations.push(op);
        }

        operations
    }

    /// Mutate an existing test case from the corpus
    fn mutate_test_case(&mut self, config: &FuzzConfig) -> Vec<Operation> {
        let corpus = match self.corpus.lock() {
            Ok(guard) => guard,
            Err(_) => {
                eprintln!("Failed to acquire corpus lock for mutation, generating random operations");
                return self.generate_random_operations(config);
            }
        };
        if corpus.is_empty() {
            drop(corpus);
            return self.generate_random_operations(config);
        }

        // Pick a random test case
        let test_case = match corpus.choose(&mut self.rng) {
            Some(tc) => tc,
            None => {
                drop(corpus);
                return self.generate_random_operations(config);
            }
        };
        let mut operations = test_case.operations.clone();
        drop(corpus);

        // Apply mutations
        let num_mutations = self.rng.gen_range(1..=5);
        for _ in 0..num_mutations {
            match self.rng.gen_range(0..5) {
                0 => {
                    // Insert a random operation
                    if !operations.is_empty() {
                        let pos = self.rng.gen_range(0..=operations.len());
                        let op = self
                            .op_generator
                            .generate_weighted(&config.operation_weights, &mut self.rng);
                        operations.insert(pos, op);
                    }
                }
                1 => {
                    // Delete an operation
                    if operations.len() > 1 {
                        let pos = self.rng.gen_range(0..operations.len());
                        operations.remove(pos);
                    }
                }
                2 => {
                    // Swap two operations
                    if operations.len() > 1 {
                        let i = self.rng.gen_range(0..operations.len());
                        let j = self.rng.gen_range(0..operations.len());
                        operations.swap(i, j);
                    }
                }
                3 => {
                    // Duplicate an operation
                    if !operations.is_empty() && operations.len() < config.max_operations {
                        let pos = self.rng.gen_range(0..operations.len());
                        let op = operations[pos].clone();
                        operations.insert(pos + 1, op);
                    }
                }
                4 => {
                    // Mutate an operation
                    if !operations.is_empty() {
                        let pos = self.rng.gen_range(0..operations.len());
                        operations[pos] = self
                            .op_generator
                            .mutate_operation(&operations[pos], &mut self.rng);
                    }
                }
                _ => unreachable!(),
            }
        }

        operations
    }

    /// Record execution results
    pub fn record_execution(
        &self,
        seed: u64,
        operations: Vec<Operation>,
        coverage: Coverage,
        found_bug: bool,
        execution_time: std::time::Duration,
    ) {
        let mut global_cov = acquire_lock!(self.global_coverage.lock(), "record execution coverage");

        // Check if this execution found new coverage
        let is_interesting = global_cov.has_new_coverage(&coverage) || found_bug;

        if is_interesting {
            // Update global coverage
            global_cov.merge(&coverage);
            drop(global_cov);

            // Add to corpus
            let test_case = TestCase {
                seed,
                operations,
                coverage,
                found_bug,
                execution_time,
            };

            acquire_lock!(self.corpus.lock(), "add interesting test case to corpus").push(test_case);
        }
    }

    /// Get corpus statistics
    pub fn corpus_stats(&self) -> CorpusStats {
        let corpus = self.corpus.lock().unwrap();
        let global_cov = self.global_coverage.lock().unwrap();

        CorpusStats {
            total_test_cases: corpus.len(),
            bug_finding_cases: corpus.iter().filter(|tc| tc.found_bug).count(),
            total_basic_blocks: global_cov.basic_blocks.len(),
            total_edges: global_cov.edges.len(),
            total_states: global_cov.states_seen.len(),
            avg_execution_time: if corpus.is_empty() {
                std::time::Duration::ZERO
            } else {
                let total: std::time::Duration = corpus.iter().map(|tc| tc.execution_time).sum();
                total / corpus.len() as u32
            },
        }
    }

    /// Set a new seed
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.rng = ChaCha8Rng::seed_from_u64(seed);
        self.op_generator.set_seed(seed);
    }
}

/// Statistics about the fuzzing corpus
#[derive(Debug)]
pub struct CorpusStats {
    pub total_test_cases: usize,
    pub bug_finding_cases: usize,
    pub total_basic_blocks: usize,
    pub total_edges: usize,
    pub total_states: usize,
    pub avg_execution_time: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_generation() {
        let mut engine = FuzzerEngine::new(42, false);
        let config = FuzzConfig {
            max_operations: 100,
            operation_weights: OperationWeights::default(),
            coverage_guided: false,
            min_nodes: 3,
            max_nodes: 5,
            fault_probability: 0.1,
            max_message_delay_ms: 1000,
        };

        let operations = engine.generate_test_case(&config).expect("Failed to generate test case");

        // Should start with node starts
        let start_ops: Vec<_> = operations
            .iter()
            .take_while(|op| matches!(op, Operation::StartNode { .. }))
            .collect();

        assert!(start_ops.len() >= config.min_nodes);
        assert!(start_ops.len() <= config.max_nodes);

        // Total operations should be within bounds
        assert!(operations.len() <= config.max_operations + config.max_nodes);
    }

    #[test]
    fn test_coverage_tracking() {
        let mut cov1 = Coverage::default();
        cov1.basic_blocks.insert(1);
        cov1.basic_blocks.insert(2);
        cov1.edges.insert((1, 2));

        let mut cov2 = Coverage::default();
        cov2.basic_blocks.insert(2);
        cov2.basic_blocks.insert(3);
        cov2.edges.insert((2, 3));

        // cov2 has new coverage
        assert!(cov1.has_new_coverage(&cov2));

        // After merging, no new coverage
        cov1.merge(&cov2);
        assert!(!cov1.has_new_coverage(&cov2));
    }
}
