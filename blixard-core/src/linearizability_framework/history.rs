//! History recording for linearizability testing
//!
//! Captures all operations with precise timing information for later analysis

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Represents a logical timestamp in the distributed system
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp {
    /// Nanoseconds since Unix epoch
    pub nanos: u64,
    /// Logical clock component for tie-breaking
    pub logical: u64,
}

impl Timestamp {
    pub fn now() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self { nanos, logical: 0 }
    }

    pub fn with_logical(mut self, logical: u64) -> Self {
        self.logical = logical;
        self
    }
}

/// VM configuration for operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VmConfig {
    pub memory_mb: u32,
    pub vcpus: u32,
    pub disk_gb: u32,
    pub image: String,
    pub features: Vec<String>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            memory_mb: 1024,
            vcpus: 1,
            disk_gb: 10,
            image: "ubuntu-22.04".to_string(),
            features: vec![],
        }
    }
}

/// Worker registration information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WorkerInfo {
    pub node_id: u64,
    pub cpu_capacity: u32,
    pub memory_capacity: u64,
    pub disk_capacity: u64,
    pub features: Vec<String>,
}

impl Default for WorkerInfo {
    fn default() -> Self {
        Self {
            node_id: 1,
            cpu_capacity: 16,
            memory_capacity: 32 * 1024 * 1024 * 1024,
            disk_capacity: 1024 * 1024 * 1024 * 1024,
            features: vec![],
        }
    }
}

/// Operation types that can be performed on the distributed system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Operation {
    // VM operations
    CreateVm {
        name: String,
        config: VmConfig,
        placement_strategy: Option<String>,
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
    MigrateVm {
        name: String,
        target_node: Option<u64>,
    },

    // Worker operations
    RegisterWorker {
        info: WorkerInfo,
    },
    UnregisterWorker {
        node_id: u64,
    },
    UpdateWorkerCapacity {
        node_id: u64,
        cpu: Option<u32>,
        memory: Option<u64>,
        disk: Option<u64>,
    },
    GetWorkerStatus {
        node_id: u64,
    },

    // Key-value operations (for testing distributed consensus)
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
    Delete {
        key: String,
    },

    // Cluster operations
    JoinCluster {
        node_id: u64,
        peer_addr: String,
    },
    LeaveCluster {
        node_id: u64,
    },
    GetClusterStatus,
    TransferLeadership {
        target_node: u64,
    },

    // Batch operations
    BatchWrite {
        writes: Vec<(String, String)>,
    },
    Transaction {
        ops: Vec<Operation>,
    },
}

/// Response types for operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Response {
    // VM responses
    VmCreated {
        success: bool,
        node_id: Option<u64>,
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
        node_id: Option<u64>,
    },
    VmMigrated {
        success: bool,
        new_node: Option<u64>,
    },

    // Worker responses
    WorkerRegistered {
        success: bool,
    },
    WorkerUnregistered {
        success: bool,
    },
    WorkerUpdated {
        success: bool,
    },
    WorkerStatus {
        online: bool,
        capacity: Option<WorkerInfo>,
        running_vms: Vec<String>,
    },

    // Key-value responses
    Value {
        value: Option<String>,
    },
    Written {
        success: bool,
    },
    Swapped {
        success: bool,
    },
    Deleted {
        success: bool,
    },

    // Cluster responses
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
        term: u64,
    },
    LeadershipTransferred {
        success: bool,
    },

    // Batch responses
    BatchResult {
        successes: usize,
        failures: usize,
    },
    TransactionResult {
        success: bool,
        results: Vec<Response>,
    },

    // Error response
    Error {
        code: String,
        message: String,
    },
}

/// A single operation in the history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub process_id: u64,
    pub operation: Operation,
    pub invocation_time: Timestamp,
    pub response_time: Option<Timestamp>,
    pub response: Option<Response>,
    pub metadata: HashMap<String, String>,
}

impl HistoryEntry {
    /// Duration of the operation
    pub fn duration(&self) -> Option<Duration> {
        self.response_time
            .map(|end| Duration::from_nanos(end.nanos - self.invocation_time.nanos))
    }

    /// Check if operation is complete
    pub fn is_complete(&self) -> bool {
        self.response.is_some()
    }

    /// Check if operation succeeded
    pub fn is_successful(&self) -> bool {
        match &self.response {
            Some(Response::Error { .. }) => false,
            Some(_) => true,
            None => false,
        }
    }
}

/// Complete history of operations
#[derive(Debug, Clone)]
pub struct History {
    pub entries: Vec<HistoryEntry>,
    next_process_id: Arc<AtomicU64>,
    logical_clock: Arc<AtomicU64>,
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_process_id: Arc::new(AtomicU64::new(0)),
            logical_clock: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get next timestamp with logical clock
    fn next_timestamp(&self) -> Timestamp {
        let logical = self.logical_clock.fetch_add(1, Ordering::SeqCst);
        Timestamp::now().with_logical(logical)
    }

    /// Start recording an operation
    pub fn begin_operation(
        &mut self,
        operation: Operation,
        metadata: HashMap<String, String>,
    ) -> u64 {
        let process_id = self.next_process_id.fetch_add(1, Ordering::SeqCst);
        self.entries.push(HistoryEntry {
            process_id,
            operation,
            invocation_time: self.next_timestamp(),
            response_time: None,
            response: None,
            metadata,
        });
        process_id
    }

    /// Complete recording an operation
    pub fn end_operation(&mut self, process_id: u64, response: Response) -> bool {
        let timestamp = self.next_timestamp();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.process_id == process_id) {
            entry.response_time = Some(timestamp);
            entry.response = Some(response);
            true
        } else {
            false
        }
    }

    /// Get all entries
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Get completed operations
    pub fn completed_operations(&self) -> Vec<&HistoryEntry> {
        self.entries.iter().filter(|e| e.is_complete()).collect()
    }

    /// Get pending operations
    pub fn pending_operations(&self) -> Vec<&HistoryEntry> {
        self.entries.iter().filter(|e| !e.is_complete()).collect()
    }

    /// Get operations by type
    pub fn operations_by_type(&self, op_type: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| match &e.operation {
                Operation::CreateVm { .. } => op_type == "CreateVm",
                Operation::StartVm { .. } => op_type == "StartVm",
                Operation::Read { .. } => op_type == "Read",
                Operation::Write { .. } => op_type == "Write",
                _ => false,
            })
            .collect()
    }

    /// Export to JSON for external analysis
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.entries)
    }

    /// Import from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let entries: Vec<HistoryEntry> = serde_json::from_str(json)?;
        let mut history = Self::new();
        history.entries = entries;
        Ok(history)
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
        self.begin_operation_with_metadata(operation, HashMap::new())
            .await
    }

    pub async fn begin_operation_with_metadata(
        &self,
        operation: Operation,
        metadata: HashMap<String, String>,
    ) -> u64 {
        self.history
            .lock()
            .await
            .begin_operation(operation, metadata)
    }

    pub async fn end_operation(&self, process_id: u64, response: Response) -> bool {
        self.history
            .lock()
            .await
            .end_operation(process_id, response)
    }

    pub async fn get_history(&self) -> History {
        self.history.lock().await.clone()
    }

    /// Record a complete operation (helper for simple cases)
    pub async fn record_operation<F, Fut>(
        &self,
        operation: Operation,
        metadata: HashMap<String, String>,
        f: F,
    ) -> Response
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Response>,
    {
        let process_id = self
            .begin_operation_with_metadata(operation, metadata)
            .await;
        let response = f().await;
        self.end_operation(process_id, response.clone()).await;
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_history_recording() {
        let recorder = HistoryRecorder::new();

        // Record a simple operation
        let op = Operation::Write {
            key: "test".to_string(),
            value: "value".to_string(),
        };

        let response = recorder
            .record_operation(op, HashMap::new(), || async {
                Response::Written { success: true }
            })
            .await;

        assert_eq!(response, Response::Written { success: true });

        let history = recorder.get_history().await;
        assert_eq!(history.entries().len(), 1);
        assert!(history.entries()[0].is_complete());
    }

    #[test]
    fn test_timestamp_ordering() {
        let t1 = Timestamp::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let t2 = Timestamp::now();

        assert!(t1 < t2);

        // Logical clock ordering
        let t3 = t1.with_logical(1);
        let t4 = t1.with_logical(2);
        assert!(t3 < t4);
    }
}
