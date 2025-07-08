//! Test case shrinking/minimization
//!
//! When a bug is found, this module helps create the minimal reproducing
//! test case by systematically removing operations while preserving the failure.

use crate::vopr::operation_generator::Operation;
use std::collections::HashSet;

/// Result of a shrinking attempt
#[derive(Debug, Clone)]
pub enum ShrinkResult {
    /// The shrunken test case still reproduces the bug
    StillFails(Vec<Operation>),
    /// The shrunken test case no longer reproduces the bug
    NoLongerFails,
    /// Shrinking was not possible (already minimal)
    CannotShrink,
}

/// Test function that returns true if the bug is reproduced
pub type TestFunction = Box<dyn Fn(&[Operation]) -> bool>;

/// Shrinker for minimizing test cases
pub struct Shrinker {
    /// Maximum iterations for shrinking
    max_iterations: usize,

    /// Whether to use aggressive shrinking strategies
    aggressive: bool,
}

impl Shrinker {
    /// Create a new shrinker
    pub fn new() -> Self {
        Self {
            max_iterations: 1000,
            aggressive: true,
        }
    }

    /// Shrink a failing test case to find minimal reproducer
    pub fn shrink(&self, operations: Vec<Operation>, test_fn: TestFunction) -> Vec<Operation> {
        // Verify the original test case fails
        if !test_fn(&operations) {
            // Original doesn't fail, return as-is
            return operations;
        }

        let mut current = operations;
        let mut iterations = 0;
        let mut made_progress = true;

        while made_progress && iterations < self.max_iterations {
            made_progress = false;
            iterations += 1;

            // Try different shrinking strategies
            let strategies: Vec<Box<dyn ShrinkStrategy>> = vec![
                Box::new(DeleteOperations::new()),
                Box::new(SimplifyOperations::new()),
                Box::new(ReorderOperations::new()),
                Box::new(CoalesceOperations::new()),
            ];

            for strategy in strategies {
                if let Some(shrunk) = strategy.apply(&current, &test_fn) {
                    println!(
                        "Shrunk from {} to {} operations using {}",
                        current.len(),
                        shrunk.len(),
                        strategy.name()
                    );
                    current = shrunk;
                    made_progress = true;
                    break; // Start over with new test case
                }
            }

            // If no strategy made progress, try more aggressive approaches
            if !made_progress && self.aggressive {
                if let Some(shrunk) = self.aggressive_shrink(&current, &test_fn) {
                    current = shrunk;
                    made_progress = true;
                }
            }
        }

        current
    }

    /// Aggressive shrinking that may take longer but finds smaller results
    fn aggressive_shrink(
        &self,
        operations: &[Operation],
        test_fn: &TestFunction,
    ) -> Option<Vec<Operation>> {
        // Try binary search deletion
        if let Some(shrunk) = self.binary_search_delete(operations, test_fn) {
            return Some(shrunk);
        }

        // Try delta debugging
        if let Some(shrunk) = self.delta_debug(operations, test_fn) {
            return Some(shrunk);
        }

        None
    }

    /// Binary search to find minimal failing subsequence
    fn binary_search_delete(
        &self,
        operations: &[Operation],
        test_fn: &TestFunction,
    ) -> Option<Vec<Operation>> {
        let n = operations.len();
        if n <= 1 {
            return None;
        }

        // Try removing first half
        let second_half: Vec<_> = operations[n / 2..].to_vec();
        if test_fn(&second_half) {
            return Some(second_half);
        }

        // Try removing second half
        let first_half: Vec<_> = operations[..n / 2].to_vec();
        if test_fn(&first_half) {
            return Some(first_half);
        }

        None
    }

    /// Delta debugging algorithm
    fn delta_debug(
        &self,
        operations: &[Operation],
        test_fn: &TestFunction,
    ) -> Option<Vec<Operation>> {
        let n = operations.len();
        if n <= 1 {
            return None;
        }

        let mut chunk_size = n / 2;

        while chunk_size >= 1 {
            let mut i = 0;
            let mut made_progress = false;

            while i + chunk_size <= n {
                // Try removing this chunk
                let mut candidate: Vec<_> = operations[..i].to_vec();
                candidate.extend_from_slice(&operations[i + chunk_size..]);

                if test_fn(&candidate) {
                    return Some(candidate);
                }

                i += chunk_size;
            }

            if !made_progress {
                chunk_size /= 2;
            }
        }

        None
    }
}

/// A shrinking strategy
trait ShrinkStrategy {
    /// Name of this strategy
    fn name(&self) -> &str;

    /// Apply the strategy and return shrunk test case if successful
    fn apply(&self, operations: &[Operation], test_fn: &TestFunction) -> Option<Vec<Operation>>;
}

/// Delete operations one by one
struct DeleteOperations {
    /// Operations that should not be deleted
    essential_ops: HashSet<String>,
}

impl DeleteOperations {
    fn new() -> Self {
        let mut essential_ops = HashSet::new();
        // Don't delete node starts as they're often required
        essential_ops.insert("StartNode".to_string());

        Self { essential_ops }
    }
}

impl ShrinkStrategy for DeleteOperations {
    fn name(&self) -> &str {
        "DeleteOperations"
    }

    fn apply(&self, operations: &[Operation], test_fn: &TestFunction) -> Option<Vec<Operation>> {
        // Try deleting each operation
        for i in 0..operations.len() {
            let op_type = match &operations[i] {
                Operation::StartNode { .. } => "StartNode",
                Operation::StopNode { .. } => "StopNode",
                Operation::ClientRequest { .. } => "ClientRequest",
                _ => "Other",
            };

            if self.essential_ops.contains(op_type) {
                continue;
            }

            let mut candidate = operations.to_vec();
            candidate.remove(i);

            if test_fn(&candidate) {
                return Some(candidate);
            }
        }

        None
    }
}

/// Simplify operations to use smaller values
struct SimplifyOperations;

impl SimplifyOperations {
    fn new() -> Self {
        Self
    }
}

impl ShrinkStrategy for SimplifyOperations {
    fn name(&self) -> &str {
        "SimplifyOperations"
    }

    fn apply(&self, operations: &[Operation], test_fn: &TestFunction) -> Option<Vec<Operation>> {
        // Try simplifying each operation
        for i in 0..operations.len() {
            let simplified = self.simplify_operation(&operations[i]);

            if let Some(simpler) = simplified {
                let mut candidate = operations.to_vec();
                candidate[i] = simpler;

                if test_fn(&candidate) {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

impl SimplifyOperations {
    fn simplify_operation(&self, op: &Operation) -> Option<Operation> {
        match op {
            Operation::ClientRequest {
                client_id,
                request_id,
                operation,
            } => {
                // Try using smaller IDs
                if *client_id > 1 {
                    return Some(Operation::ClientRequest {
                        client_id: 1,
                        request_id: *request_id,
                        operation: operation.clone(),
                    });
                }
            }
            Operation::ClockJump { node_id, delta_ms } => {
                // Try smaller jumps
                if delta_ms.abs() > 1000 {
                    return Some(Operation::ClockJump {
                        node_id: *node_id,
                        delta_ms: delta_ms.signum() * 1000,
                    });
                }
            }
            Operation::NetworkDelay { from, to, delay_ms } => {
                // Try smaller delays
                if *delay_ms > 100 {
                    return Some(Operation::NetworkDelay {
                        from: *from,
                        to: *to,
                        delay_ms: 100,
                    });
                }
            }
            _ => {}
        }

        None
    }
}

/// Reorder operations to find simpler sequences
struct ReorderOperations;

impl ReorderOperations {
    fn new() -> Self {
        Self
    }
}

impl ShrinkStrategy for ReorderOperations {
    fn name(&self) -> &str {
        "ReorderOperations"
    }

    fn apply(&self, operations: &[Operation], test_fn: &TestFunction) -> Option<Vec<Operation>> {
        if operations.len() < 2 {
            return None;
        }

        // Try swapping adjacent operations
        for i in 0..operations.len() - 1 {
            if self.can_swap(&operations[i], &operations[i + 1]) {
                let mut candidate = operations.to_vec();
                candidate.swap(i, i + 1);

                if test_fn(&candidate) {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

impl ReorderOperations {
    fn can_swap(&self, op1: &Operation, op2: &Operation) -> bool {
        // Don't swap operations that have dependencies
        match (op1, op2) {
            (Operation::StartNode { node_id: id1 }, Operation::StopNode { node_id: id2 }) => {
                id1 != id2
            }
            (Operation::StartNode { .. }, Operation::StartNode { .. }) => true,
            (Operation::ClientRequest { .. }, Operation::ClientRequest { .. }) => true,
            _ => true,
        }
    }
}

/// Coalesce similar operations
struct CoalesceOperations;

impl CoalesceOperations {
    fn new() -> Self {
        Self
    }
}

impl ShrinkStrategy for CoalesceOperations {
    fn name(&self) -> &str {
        "CoalesceOperations"
    }

    fn apply(&self, operations: &[Operation], test_fn: &TestFunction) -> Option<Vec<Operation>> {
        let mut i = 0;
        while i < operations.len() - 1 {
            if self.can_coalesce(&operations[i], &operations[i + 1]) {
                let mut candidate = operations.to_vec();
                // Remove the second operation
                candidate.remove(i + 1);

                if test_fn(&candidate) {
                    return Some(candidate);
                }
            }
            i += 1;
        }

        None
    }
}

impl CoalesceOperations {
    fn can_coalesce(&self, op1: &Operation, op2: &Operation) -> bool {
        match (op1, op2) {
            // Multiple stops of same node can be coalesced
            (Operation::StopNode { node_id: id1 }, Operation::StopNode { node_id: id2 }) => {
                id1 == id2
            }
            // Multiple clock jumps on same node can be combined
            (
                Operation::ClockJump { node_id: id1, .. },
                Operation::ClockJump { node_id: id2, .. },
            ) => id1 == id2,
            _ => false,
        }
    }
}

impl Default for Shrinker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_shrinking() {
        let operations = vec![
            Operation::StartNode { node_id: 1 },
            Operation::StartNode { node_id: 2 },
            Operation::ClockJump {
                node_id: 1,
                delta_ms: 5000,
            },
            Operation::StopNode { node_id: 2 },
            Operation::StartNode { node_id: 3 },
        ];

        // Test function that fails if operation 2 (clock jump) is present
        let test_fn: TestFunction = Box::new(|ops| {
            ops.iter()
                .any(|op| matches!(op, Operation::ClockJump { .. }))
        });

        let shrinker = Shrinker::new();
        let minimal = shrinker.shrink(operations, test_fn);

        // Should reduce to just the clock jump
        assert_eq!(minimal.len(), 1);
        assert!(matches!(minimal[0], Operation::ClockJump { .. }));
    }
}
