//! Linearizability testing support for Blixard
//!
//! This module provides integration between the linearizability testing framework
//! and actual Blixard operations.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    error::BlixardResult,
    iroh_types::{
        CreateVmRequest, CreateVmResponse, GetVmStatusRequest, GetVmStatusResponse,
        ListVmsResponse, Response, StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse,
        VmConfig, VmInfo, VmState,
    },
    transport::iroh_client::IrohClusterServiceClient,
};

/// Operation history for linearizability testing
#[derive(Debug, Clone)]
pub struct OperationHistory {
    entries: Arc<Mutex<Vec<HistoryEntry>>>,
    next_id: Arc<Mutex<u64>>,
}

/// A single operation in the history
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: u64,
    pub operation: OperationType,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub result: Option<OperationResult>,
    pub client_id: u64,
}

/// Types of operations that can be recorded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    CreateVm { name: String },
    StartVm { name: String },
    StopVm { name: String },
    DeleteVm { name: String },
    ListVms,
    GetVmStatus { name: String },
    JoinCluster { node_id: u64 },
    LeaveCluster { node_id: u64 },
    GetClusterStatus,
}

/// Results of operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationResult {
    Success,
    Failure {
        error: String,
    },
    VmList {
        vms: Vec<String>,
    },
    VmStatus {
        exists: bool,
        running: bool,
    },
    ClusterStatus {
        leader: Option<u64>,
        nodes: Vec<u64>,
    },
}

impl OperationHistory {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Start recording an operation
    pub async fn begin(&self, operation: OperationType, client_id: u64) -> u64 {
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;

        let entry = HistoryEntry {
            id,
            operation,
            start_time: Instant::now(),
            end_time: None,
            result: None,
            client_id,
        };

        self.entries.lock().await.push(entry);
        id
    }

    /// Complete recording an operation
    pub async fn complete(&self, id: u64, result: OperationResult) {
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
            entry.end_time = Some(Instant::now());
            entry.result = Some(result);
        }
    }

    /// Get all completed operations
    pub async fn get_completed(&self) -> Vec<HistoryEntry> {
        self.entries
            .lock()
            .await
            .iter()
            .filter(|e| e.result.is_some())
            .cloned()
            .collect()
    }

    /// Export history for analysis
    pub async fn export(&self) -> Vec<HistoryEntry> {
        self.entries.lock().await.clone()
    }
}

/// A client wrapper that records operations for linearizability testing
pub struct RecordingClient {
    inner: IrohClusterServiceClient,
    history: OperationHistory,
    client_id: u64,
}

impl RecordingClient {
    pub fn new(
        client: IrohClusterServiceClient,
        history: OperationHistory,
        client_id: u64,
    ) -> Self {
        Self {
            inner: client,
            history,
            client_id,
        }
    }

    /// Create a VM with history recording
    pub async fn create_vm(&self, config: VmConfig) -> BlixardResult<Response<CreateVmResponse>> {
        let op_id = self
            .history
            .begin(
                OperationType::CreateVm {
                    name: config.name.clone(),
                },
                self.client_id,
            )
            .await;

        let result = self.inner.create_vm(config).await;

        let op_result = match &result {
            Ok(resp) => {
                if resp.get_ref().success {
                    OperationResult::Success
                } else {
                    OperationResult::Failure {
                        error: resp.get_ref().message.clone(),
                    }
                }
            }
            Err(e) => OperationResult::Failure {
                error: e.to_string(),
            },
        };

        self.history.complete(op_id, op_result).await;
        result
    }

    /// Start a VM with history recording
    pub async fn start_vm(&self, name: String) -> BlixardResult<Response<StartVmResponse>> {
        let op_id = self
            .history
            .begin(
                OperationType::StartVm { name: name.clone() },
                self.client_id,
            )
            .await;

        let result = self.inner.start_vm(StartVmRequest { name }).await;

        let op_result = match &result {
            Ok(resp) => {
                if resp.get_ref().success {
                    OperationResult::Success
                } else {
                    OperationResult::Failure {
                        error: resp.get_ref().message.clone(),
                    }
                }
            }
            Err(e) => OperationResult::Failure {
                error: e.to_string(),
            },
        };

        self.history.complete(op_id, op_result).await;
        result
    }

    /// Stop a VM with history recording
    pub async fn stop_vm(&self, name: String) -> BlixardResult<Response<StopVmResponse>> {
        let op_id = self
            .history
            .begin(OperationType::StopVm { name: name.clone() }, self.client_id)
            .await;

        let result = self.inner.stop_vm(StopVmRequest { name }).await;

        let op_result = match &result {
            Ok(resp) => {
                if resp.get_ref().success {
                    OperationResult::Success
                } else {
                    OperationResult::Failure {
                        error: resp.get_ref().message.clone(),
                    }
                }
            }
            Err(e) => OperationResult::Failure {
                error: e.to_string(),
            },
        };

        self.history.complete(op_id, op_result).await;
        result
    }

    /// List VMs with history recording
    pub async fn list_vms(&self) -> BlixardResult<Vec<VmInfo>> {
        let op_id = self
            .history
            .begin(OperationType::ListVms, self.client_id)
            .await;

        let result = self.inner.list_vms().await;

        let op_result = match &result {
            Ok(vms) => OperationResult::VmList {
                vms: vms.iter().map(|v| v.name.clone()).collect(),
            },
            Err(e) => OperationResult::Failure {
                error: e.to_string(),
            },
        };

        self.history.complete(op_id, op_result).await;
        result
    }

    /// Get VM status with history recording
    pub async fn get_vm_status(&self, name: String) -> BlixardResult<Option<VmInfo>> {
        let op_id = self
            .history
            .begin(
                OperationType::GetVmStatus { name: name.clone() },
                self.client_id,
            )
            .await;

        let result = self.inner.get_vm_status(name).await;

        let op_result = match &result {
            Ok(vm_info_opt) => {
                if let Some(info) = vm_info_opt {
                    OperationResult::VmStatus {
                        exists: true,
                        running: info.state == 3, // Running state
                    }
                } else {
                    OperationResult::VmStatus {
                        exists: false,
                        running: false,
                    }
                }
            }
            Err(e) => OperationResult::Failure {
                error: e.to_string(),
            },
        };

        self.history.complete(op_id, op_result).await;
        result
    }
}

/// Linearizability checker that can analyze recorded histories
pub struct LinearizabilityChecker {
    history: OperationHistory,
}

impl LinearizabilityChecker {
    pub fn new(history: OperationHistory) -> Self {
        Self { history }
    }

    /// Check if the recorded history is linearizable
    pub async fn check(&self) -> Result<(), String> {
        let entries = self.history.get_completed().await;

        // Convert to a format suitable for linearizability checking
        // This would integrate with the linearizability framework

        // For now, perform basic consistency checks
        self.check_vm_consistency(&entries)?;
        self.check_cluster_consistency(&entries)?;

        Ok(())
    }

    fn check_vm_consistency(&self, entries: &[HistoryEntry]) -> Result<(), String> {
        // Check that VMs are not created twice with success
        let mut created_vms = std::collections::HashSet::new();

        for entry in entries {
            match (&entry.operation, &entry.result) {
                (OperationType::CreateVm { name }, Some(OperationResult::Success)) => {
                    if !created_vms.insert(name.clone()) {
                        return Err(format!("VM '{}' created successfully multiple times", name));
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn check_cluster_consistency(&self, entries: &[HistoryEntry]) -> Result<(), String> {
        // Check that cluster operations maintain consistency
        // This is a simplified check - real implementation would be more thorough

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_operation_history_recording() {
        let history = OperationHistory::new();

        // Record a successful operation
        let id = history
            .begin(
                OperationType::CreateVm {
                    name: "test-vm".to_string(),
                },
                1,
            )
            .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        history.complete(id, OperationResult::Success).await;

        // Check recorded history
        let completed = history.get_completed().await;
        assert_eq!(completed.len(), 1);
        assert!(completed[0].end_time.is_some());
        assert!(matches!(
            completed[0].result,
            Some(OperationResult::Success)
        ));
    }
}
