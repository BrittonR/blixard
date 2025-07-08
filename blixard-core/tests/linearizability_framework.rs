//! Linearizability testing framework for distributed operations
//!
//! This framework provides tools to verify that concurrent operations on the distributed
//! system appear to take effect atomically at some point between their invocation and response.
//!
//! Inspired by Jepsen's Elle and Porcupine linearizability checkers.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Operation types that can be performed on the distributed system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Operation {
    /// VM operations
    CreateVm {
        name: String,
        config: VmConfig,
    },
    StartVm {
        name: String,
    },
    StopVm {
        name: String,
    },
    DeleteVm {
        name: String,
    },
    GetVmStatus {
        name: String,
    },

    /// Key-value operations (for testing distributed consensus)
    Read {
        key: String,
    },
    Write {
        key: String,
        value: String,
    },
    CompareAndSwap {
        key: String,
        old: String,
        new: String,
    },

    /// Cluster operations
    JoinCluster {
        node_id: u64,
    },
    LeaveCluster {
        node_id: u64,
    },
    GetClusterStatus,
}

/// Response types for operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Response {
    /// VM responses
    VmCreated {
        success: bool,
    },
    VmStarted {
        success: bool,
    },
    VmStopped {
        success: bool,
    },
    VmDeleted {
        success: bool,
    },
    VmStatus {
        exists: bool,
        state: Option<String>,
    },

    /// Key-value responses
    Value {
        value: Option<String>,
    },
    Written {
        success: bool,
    },
    Swapped {
        success: bool,
    },

    /// Cluster responses
    Joined {
        success: bool,
        peers: Vec<u64>,
    },
    Left {
        success: bool,
    },
    ClusterStatus {
        leader: Option<u64>,
        nodes: Vec<u64>,
    },

    /// Error response
    Error {
        message: String,
    },
}

/// A single operation in the history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub process_id: u64,
    pub operation: Operation,
    pub invocation_time: Instant,
    pub response_time: Option<Instant>,
    pub response: Option<Response>,
}

/// Complete history of operations
#[derive(Debug, Clone)]
pub struct History {
    entries: Vec<HistoryEntry>,
    next_process_id: AtomicU64,
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_process_id: AtomicU64::new(0),
        }
    }

    /// Start recording an operation
    pub fn begin_operation(&mut self, operation: Operation) -> u64 {
        let process_id = self.next_process_id.fetch_add(1, Ordering::SeqCst);
        self.entries.push(HistoryEntry {
            process_id,
            operation,
            invocation_time: Instant::now(),
            response_time: None,
            response: None,
        });
        process_id
    }

    /// Complete recording an operation
    pub fn end_operation(&mut self, process_id: u64, response: Response) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.process_id == process_id) {
            entry.response_time = Some(Instant::now());
            entry.response = Some(response);
        }
    }

    /// Get completed operations
    pub fn completed_operations(&self) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.response.is_some())
            .collect()
    }
}

/// Thread-safe history recorder
#[derive(Clone)]
pub struct HistoryRecorder {
    history: Arc<Mutex<History>>,
}

impl HistoryRecorder {
    pub fn new() -> Self {
        Self {
            history: Arc::new(Mutex::new(History::new())),
        }
    }

    pub async fn begin_operation(&self, operation: Operation) -> u64 {
        self.history.lock().await.begin_operation(operation)
    }

    pub async fn end_operation(&self, process_id: u64, response: Response) {
        self.history
            .lock()
            .await
            .end_operation(process_id, response)
    }

    pub async fn get_history(&self) -> History {
        self.history.lock().await.clone()
    }
}

/// Sequential specification for VM operations
pub struct VmSpecification {
    vms: HashMap<String, VmState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VmState {
    config: VmConfig,
    running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VmConfig {
    pub memory_mb: u32,
    pub vcpus: u32,
}

impl VmSpecification {
    pub fn new() -> Self {
        Self {
            vms: HashMap::new(),
        }
    }

    /// Apply an operation to the sequential specification
    pub fn apply(&mut self, operation: &Operation) -> Response {
        match operation {
            Operation::CreateVm { name, config } => {
                if self.vms.contains_key(name) {
                    Response::VmCreated { success: false }
                } else {
                    self.vms.insert(
                        name.clone(),
                        VmState {
                            config: config.clone(),
                            running: false,
                        },
                    );
                    Response::VmCreated { success: true }
                }
            }
            Operation::StartVm { name } => {
                if let Some(vm) = self.vms.get_mut(name) {
                    vm.running = true;
                    Response::VmStarted { success: true }
                } else {
                    Response::VmStarted { success: false }
                }
            }
            Operation::StopVm { name } => {
                if let Some(vm) = self.vms.get_mut(name) {
                    vm.running = false;
                    Response::VmStopped { success: true }
                } else {
                    Response::VmStopped { success: false }
                }
            }
            Operation::DeleteVm { name } => {
                if self.vms.remove(name).is_some() {
                    Response::VmDeleted { success: true }
                } else {
                    Response::VmDeleted { success: false }
                }
            }
            Operation::GetVmStatus { name } => {
                if let Some(vm) = self.vms.get(name) {
                    Response::VmStatus {
                        exists: true,
                        state: Some(if vm.running { "running" } else { "stopped" }.to_string()),
                    }
                } else {
                    Response::VmStatus {
                        exists: false,
                        state: None,
                    }
                }
            }
            _ => Response::Error {
                message: "Unsupported operation".to_string(),
            },
        }
    }
}

/// Check if a history is linearizable with respect to a specification
pub fn check_linearizability<S, F>(
    history: &History,
    spec_factory: F,
) -> Result<(), LinearizabilityError>
where
    S: Specification,
    F: Fn() -> S,
{
    let completed = history.completed_operations();
    if completed.is_empty() {
        return Ok(());
    }

    // Simple implementation: try all possible linearizations
    // In production, use more efficient algorithms like P-compositionality
    let mut permutation = (0..completed.len()).collect::<Vec<_>>();

    loop {
        let mut spec = spec_factory();
        let mut valid = true;

        for &idx in &permutation {
            let entry = completed[idx];
            let expected = spec.apply(&entry.operation);
            if entry.response.as_ref() != Some(&expected) {
                valid = false;
                break;
            }
        }

        if valid {
            return Ok(());
        }

        // Next permutation
        if !next_permutation(&mut permutation) {
            break;
        }
    }

    Err(LinearizabilityError::NoValidLinearization)
}

/// Trait for sequential specifications
pub trait Specification {
    fn apply(&mut self, operation: &Operation) -> Response;
}

impl Specification for VmSpecification {
    fn apply(&mut self, operation: &Operation) -> Response {
        self.apply(operation)
    }
}

/// Key-value store specification
pub struct KvSpecification {
    store: HashMap<String, String>,
}

impl KvSpecification {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Specification for KvSpecification {
    fn apply(&mut self, operation: &Operation) -> Response {
        match operation {
            Operation::Read { key } => Response::Value {
                value: self.store.get(key).cloned(),
            },
            Operation::Write { key, value } => {
                self.store.insert(key.clone(), value.clone());
                Response::Written { success: true }
            }
            Operation::CompareAndSwap { key, old, new } => {
                if self.store.get(key) == Some(old) {
                    self.store.insert(key.clone(), new.clone());
                    Response::Swapped { success: true }
                } else {
                    Response::Swapped { success: false }
                }
            }
            _ => Response::Error {
                message: "Unsupported operation".to_string(),
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LinearizabilityError {
    #[error("No valid linearization found")]
    NoValidLinearization,
}

/// Generate next permutation (lexicographic order)
fn next_permutation(arr: &mut [usize]) -> bool {
    if arr.len() <= 1 {
        return false;
    }

    let mut i = arr.len() - 1;
    while i > 0 && arr[i - 1] >= arr[i] {
        i -= 1;
    }

    if i == 0 {
        return false;
    }

    let mut j = arr.len() - 1;
    while arr[j] <= arr[i - 1] {
        j -= 1;
    }

    arr.swap(i - 1, j);
    arr[i..].reverse();
    true
}

/// History analyzer that provides detailed analysis of linearizability violations
pub struct HistoryAnalyzer {
    history: History,
}

impl HistoryAnalyzer {
    pub fn new(history: History) -> Self {
        Self { history }
    }

    /// Find minimal failing subsequence
    pub fn find_minimal_violation<S, F>(&self, spec_factory: F) -> Option<Vec<HistoryEntry>>
    where
        S: Specification,
        F: Fn() -> S + Clone,
    {
        let completed = self.history.completed_operations();

        // Try to find minimal subsequence that violates linearizability
        for size in 2..=completed.len() {
            for window in completed.windows(size) {
                let sub_history = History {
                    entries: window.to_vec().into_iter().cloned().collect(),
                    next_process_id: AtomicU64::new(0),
                };

                if check_linearizability(&sub_history, spec_factory.clone()).is_err() {
                    return Some(window.to_vec().into_iter().cloned().collect());
                }
            }
        }

        None
    }

    /// Detect common patterns of linearizability violations
    pub fn detect_violation_patterns(&self) -> Vec<ViolationPattern> {
        let mut patterns = Vec::new();

        // Check for lost updates
        if self.detect_lost_updates() {
            patterns.push(ViolationPattern::LostUpdate);
        }

        // Check for dirty reads
        if self.detect_dirty_reads() {
            patterns.push(ViolationPattern::DirtyRead);
        }

        // Check for non-repeatable reads
        if self.detect_non_repeatable_reads() {
            patterns.push(ViolationPattern::NonRepeatableRead);
        }

        patterns
    }

    fn detect_lost_updates(&self) -> bool {
        // Simplified detection logic
        // In practice, implement more sophisticated pattern matching
        false
    }

    fn detect_dirty_reads(&self) -> bool {
        false
    }

    fn detect_non_repeatable_reads(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationPattern {
    LostUpdate,
    DirtyRead,
    NonRepeatableRead,
    PhantomRead,
    WriteSkew,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_linearizable_history() {
        let mut history = History::new();

        // Sequential operations
        let id1 = history.begin_operation(Operation::CreateVm {
            name: "vm1".to_string(),
            config: VmConfig {
                memory_mb: 1024,
                vcpus: 2,
            },
        });
        history.end_operation(id1, Response::VmCreated { success: true });

        let id2 = history.begin_operation(Operation::StartVm {
            name: "vm1".to_string(),
        });
        history.end_operation(id2, Response::VmStarted { success: true });

        assert!(check_linearizability(&history, VmSpecification::new).is_ok());
    }

    #[tokio::test]
    async fn test_non_linearizable_history() {
        let mut history = History::new();

        // Create same VM twice with both succeeding (impossible)
        let id1 = history.begin_operation(Operation::CreateVm {
            name: "vm1".to_string(),
            config: VmConfig {
                memory_mb: 1024,
                vcpus: 2,
            },
        });
        history.end_operation(id1, Response::VmCreated { success: true });

        let id2 = history.begin_operation(Operation::CreateVm {
            name: "vm1".to_string(),
            config: VmConfig {
                memory_mb: 2048,
                vcpus: 4,
            },
        });
        history.end_operation(id2, Response::VmCreated { success: true });

        assert!(check_linearizability(&history, VmSpecification::new).is_err());
    }
}
