//! Operation generator for creating valid but challenging test sequences
//!
//! This module generates operations that stress the distributed system,
//! including normal operations, fault injections, and Byzantine behaviors.

use crate::vopr::fuzzer_engine::OperationWeights;
use crate::unwrap_helpers::choose_random;
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// An operation that can be performed on the system
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    // Node lifecycle operations
    StartNode {
        node_id: u64,
    },
    StopNode {
        node_id: u64,
    },
    RestartNode {
        node_id: u64,
    },

    // Client operations
    ClientRequest {
        client_id: u64,
        request_id: u64,
        operation: ClientOp,
    },

    // Network operations
    NetworkPartition {
        partition_a: Vec<u64>,
        partition_b: Vec<u64>,
    },
    NetworkHeal,
    NetworkDelay {
        from: u64,
        to: u64,
        delay_ms: u64,
    },

    // Message fault injection
    DropMessage {
        from: u64,
        to: u64,
        probability: f64,
    },
    DuplicateMessage {
        from: u64,
        to: u64,
        count: u32,
    },
    ReorderMessages {
        from: u64,
        to: u64,
        window_size: u32,
    },

    // Time manipulation
    ClockJump {
        node_id: u64,
        delta_ms: i64,
    },
    ClockDrift {
        node_id: u64,
        drift_rate_ppm: i64, // parts per million
    },

    // Byzantine behaviors
    ByzantineNode {
        node_id: u64,
        behavior: ByzantineBehavior,
    },

    // State verification
    CheckInvariant {
        invariant: String,
    },

    // Wait for condition
    WaitForProgress {
        timeout_ms: u64,
    },
}

/// Client operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientOp {
    // VM operations
    CreateVm {
        vm_id: String,
        cpu: u32,
        memory: u32,
    },
    StartVm {
        vm_id: String,
    },
    StopVm {
        vm_id: String,
    },
    DeleteVm {
        vm_id: String,
    },

    // Task operations
    SubmitTask {
        task_id: String,
        command: String,
    },

    // Read operations
    GetVmStatus {
        vm_id: String,
    },
    GetClusterStatus,
    ListVms,
}

/// Byzantine node behaviors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByzantineBehavior {
    // Send conflicting messages
    Equivocation,

    // Vote for multiple candidates in same term
    MultipleVotes,

    // Claim to be leader without election
    FakeLeader,

    // Send messages with wrong term
    WrongTerm,

    // Corrupt message contents
    CorruptMessages,

    // Replay old messages
    ReplayAttack,

    // Pretend to be a different node
    IdentityForgery { pretend_id: u64 },

    // Stop responding (undetectable failure)
    Silent,
}

/// Operation generator that creates test sequences
pub struct OperationGenerator {
    rng: ChaCha8Rng,
    next_client_id: u64,
    next_request_id: u64,
    next_vm_id: u64,
    active_nodes: Vec<u64>,
    active_vms: Vec<String>,
}

impl OperationGenerator {
    /// Create a new operation generator
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            next_client_id: 1,
            next_request_id: 1,
            next_vm_id: 1,
            active_nodes: Vec::new(),
            active_vms: Vec::new(),
        }
    }

    /// Set a new seed
    pub fn set_seed(&mut self, seed: u64) {
        self.rng = ChaCha8Rng::seed_from_u64(seed);
    }

    /// Generate an operation based on weights
    pub fn generate_weighted(
        &mut self,
        weights: &OperationWeights,
        rng: &mut ChaCha8Rng,
    ) -> Operation {
        let total_weight = weights.start_node
            + weights.stop_node
            + weights.restart_node
            + weights.client_request
            + weights.network_partition
            + weights.network_heal
            + weights.clock_jump
            + weights.clock_drift
            + weights.message_drop
            + weights.message_duplicate
            + weights.message_reorder
            + weights.byzantine_behavior;

        let mut choice = rng.gen_range(0.0..total_weight);

        // Subtract weights until we find our choice
        macro_rules! check_weight {
            ($weight:expr, $op:expr) => {
                if choice < $weight {
                    return $op;
                }
                choice -= $weight;
            };
        }

        check_weight!(weights.start_node, self.generate_start_node());
        check_weight!(weights.stop_node, self.generate_stop_node(rng));
        check_weight!(weights.restart_node, self.generate_restart_node(rng));
        check_weight!(weights.client_request, self.generate_client_request(rng));
        check_weight!(
            weights.network_partition,
            self.generate_network_partition(rng)
        );
        check_weight!(weights.network_heal, Operation::NetworkHeal);
        check_weight!(weights.clock_jump, self.generate_clock_jump(rng));
        check_weight!(weights.clock_drift, self.generate_clock_drift(rng));
        check_weight!(weights.message_drop, self.generate_message_drop(rng));
        check_weight!(
            weights.message_duplicate,
            self.generate_message_duplicate(rng)
        );
        check_weight!(weights.message_reorder, self.generate_message_reorder(rng));
        check_weight!(
            weights.byzantine_behavior,
            self.generate_byzantine_behavior(rng)
        );

        // Fallback - should have exhausted all weights
        debug_assert!(choice < 0.01, "Unexpected remaining weight: {}", choice);
        self.generate_client_request(rng)
    }

    /// Mutate an existing operation
    pub fn mutate_operation(&mut self, op: &Operation, rng: &mut ChaCha8Rng) -> Operation {
        match op {
            Operation::StartNode { .. } => {
                // Change to stopping a random node
                match choose_random(&self.active_nodes) {
                    Ok(node_id) => Operation::StopNode { node_id: *node_id },
                    Err(_) => op.clone(), // No active nodes available
                }
            }
            Operation::ClientRequest { client_id, .. } => {
                // Generate a new request from the same client
                Operation::ClientRequest {
                    client_id: *client_id,
                    request_id: self.next_request_id(),
                    operation: self.generate_client_op(rng),
                }
            }
            Operation::ClockJump { node_id, delta_ms } => {
                // Vary the jump amount
                let new_delta = if rng.gen_bool(0.5) {
                    delta_ms.saturating_mul(2)
                } else {
                    delta_ms / 2
                };
                Operation::ClockJump {
                    node_id: *node_id,
                    delta_ms: new_delta,
                }
            }
            _ => {
                // For other operations, generate a random one
                self.generate_weighted(&OperationWeights::default(), rng)
            }
        }
    }

    // Operation generators

    fn generate_start_node(&mut self) -> Operation {
        let node_id = self.active_nodes.len() as u64 + 1;
        self.active_nodes.push(node_id);
        Operation::StartNode { node_id }
    }

    fn generate_stop_node(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.is_empty() {
            return self.generate_start_node();
        }

        let idx = rng.gen_range(0..self.active_nodes.len());
        let node_id = self.active_nodes.remove(idx);
        Operation::StopNode { node_id }
    }

    fn generate_restart_node(&mut self, _rng: &mut ChaCha8Rng) -> Operation {
        match choose_random(&self.active_nodes) {
            Ok(node_id) => Operation::RestartNode { node_id: *node_id },
            Err(_) => self.generate_start_node(), // No active nodes, start a new one
        }
    }

    fn generate_client_request(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        let client_id = if rng.gen_bool(0.7) && self.next_client_id > 1 {
            // Reuse existing client
            rng.gen_range(1..self.next_client_id)
        } else {
            // New client
            let id = self.next_client_id;
            self.next_client_id += 1;
            id
        };

        Operation::ClientRequest {
            client_id,
            request_id: self.next_request_id(),
            operation: self.generate_client_op(rng),
        }
    }

    fn generate_client_op(&mut self, rng: &mut ChaCha8Rng) -> ClientOp {
        match rng.gen_range(0..7) {
            0 => {
                let vm_id = format!("vm-{}", self.next_vm_id);
                self.next_vm_id += 1;
                self.active_vms.push(vm_id.clone());

                ClientOp::CreateVm {
                    vm_id,
                    cpu: rng.gen_range(1..=8),
                    memory: rng.gen_range(512..=8192),
                }
            }
            1 if !self.active_vms.is_empty() => {
                let vm_id = match self.active_vms.choose(rng) {
                    Some(id) => id.clone(),
                    None => return self.generate_create_vm(),
                };
                ClientOp::StartVm { vm_id }
            }
            2 if !self.active_vms.is_empty() => {
                let vm_id = match self.active_vms.choose(rng) {
                    Some(id) => id.clone(),
                    None => return self.generate_create_vm(),
                };
                ClientOp::StopVm { vm_id }
            }
            3 if !self.active_vms.is_empty() => {
                let idx = rng.gen_range(0..self.active_vms.len());
                let vm_id = self.active_vms.remove(idx);
                ClientOp::DeleteVm { vm_id }
            }
            4 => ClientOp::SubmitTask {
                task_id: format!("task-{}", rng.gen::<u32>()),
                command: "echo test".to_string(),
            },
            5 if !self.active_vms.is_empty() => {
                let vm_id = match self.active_vms.choose(rng) {
                    Some(id) => id.clone(),
                    None => return self.generate_create_vm(),
                };
                ClientOp::GetVmStatus { vm_id }
            }
            _ => ClientOp::GetClusterStatus,
        }
    }

    fn generate_network_partition(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.len() < 2 {
            return self.generate_start_node();
        }

        // Randomly partition the nodes
        let mut nodes = self.active_nodes.clone();
        nodes.shuffle(rng);

        let split_point = rng.gen_range(1..nodes.len());
        let partition_a = nodes[..split_point].to_vec();
        let partition_b = nodes[split_point..].to_vec();

        Operation::NetworkPartition {
            partition_a,
            partition_b,
        }
    }

    fn generate_clock_jump(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.is_empty() {
            return self.generate_start_node();
        }

        let node_id = match self.active_nodes.choose(rng) {
            Some(id) => *id,
            None => return self.generate_start_node(),
        };
        let delta_ms = rng.gen_range(-60000..=60000); // ±1 minute

        Operation::ClockJump { node_id, delta_ms }
    }

    fn generate_clock_drift(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.is_empty() {
            return self.generate_start_node();
        }

        let node_id = match self.active_nodes.choose(rng) {
            Some(id) => *id,
            None => return self.generate_start_node(),
        };
        let drift_rate_ppm = rng.gen_range(-10000..=10000); // ±10,000 ppm = ±1%

        Operation::ClockDrift {
            node_id,
            drift_rate_ppm,
        }
    }

    fn generate_message_drop(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.len() < 2 {
            return self.generate_start_node();
        }

        let from = match self.active_nodes.choose(rng) {
            Some(id) => *id,
            None => return self.generate_start_node(),
        };
        let to = loop {
            let node = match self.active_nodes.choose(rng) {
                Some(id) => *id,
                None => continue,  // This shouldn't happen but handle gracefully
            };
            if node != from {
                break node;
            }
        };

        let probability = rng.gen_range(0.1..=0.9);

        Operation::DropMessage {
            from,
            to,
            probability,
        }
    }

    fn generate_message_duplicate(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.len() < 2 {
            return self.generate_start_node();
        }

        let from = match self.active_nodes.choose(rng) {
            Some(id) => *id,
            None => return self.generate_start_node(),
        };
        let to = loop {
            let node = match self.active_nodes.choose(rng) {
                Some(id) => *id,
                None => continue,  // This shouldn't happen but handle gracefully
            };
            if node != from {
                break node;
            }
        };

        let count = rng.gen_range(2..=5);

        Operation::DuplicateMessage { from, to, count }
    }

    fn generate_message_reorder(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.len() < 2 {
            return self.generate_start_node();
        }

        let from = match self.active_nodes.choose(rng) {
            Some(id) => *id,
            None => return self.generate_start_node(),
        };
        let to = loop {
            let node = match self.active_nodes.choose(rng) {
                Some(id) => *id,
                None => continue,  // This shouldn't happen but handle gracefully
            };
            if node != from {
                break node;
            }
        };

        let window_size = rng.gen_range(2..=10);

        Operation::ReorderMessages {
            from,
            to,
            window_size,
        }
    }

    fn generate_byzantine_behavior(&mut self, rng: &mut ChaCha8Rng) -> Operation {
        if self.active_nodes.is_empty() {
            return self.generate_start_node();
        }

        let node_id = match self.active_nodes.choose(rng) {
            Some(id) => *id,
            None => return self.generate_start_node(),
        };

        let behavior = match rng.gen_range(0..9) {
            0 => ByzantineBehavior::Equivocation,
            1 => ByzantineBehavior::MultipleVotes,
            2 => ByzantineBehavior::FakeLeader,
            3 => ByzantineBehavior::WrongTerm,
            4 => ByzantineBehavior::CorruptMessages,
            5 => ByzantineBehavior::ReplayAttack,
            6 => {
                let pretend_id = loop {
                    let id = match self.active_nodes.choose(rng) {
                        Some(node_id) => *node_id,
                        None => continue,  // This shouldn't happen but handle gracefully
                    };
                    if id != node_id {
                        break id;
                    }
                };
                ByzantineBehavior::IdentityForgery { pretend_id }
            }
            7 => ByzantineBehavior::Silent,
            _ => ByzantineBehavior::Equivocation,
        };

        Operation::ByzantineNode { node_id, behavior }
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    /// Generate a CreateVm operation (helper method)
    fn generate_create_vm(&mut self) -> ClientOp {
        let vm_id = format!("vm-{}", self.next_vm_id);
        self.next_vm_id += 1;
        self.active_vms.push(vm_id.clone());

        ClientOp::CreateVm {
            vm_id,
            cpu: 2,    // Default CPU count
            memory: 1024, // Default memory in MB
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_generation() {
        let mut gen = OperationGenerator::new(42);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // Generate some operations
        let weights = OperationWeights::default();

        let ops: Vec<_> = (0..100)
            .map(|_| gen.generate_weighted(&weights, &mut rng))
            .collect();

        // Should have a mix of operations
        let has_client_ops = ops
            .iter()
            .any(|op| matches!(op, Operation::ClientRequest { .. }));
        assert!(has_client_ops);
    }

    #[test]
    fn test_operation_mutation() {
        let mut gen = OperationGenerator::new(42);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // Start with a start node operation
        let op = Operation::StartNode { node_id: 1 };
        gen.active_nodes.push(1);

        // Mutate it
        let mutated = gen.mutate_operation(&op, &mut rng);

        // Should be different
        assert_ne!(op, mutated);
    }
}
