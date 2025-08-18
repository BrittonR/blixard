//! Quota service implementation for Iroh transport
//!
//! This service handles quota management operations over Iroh transport.

use crate::{
    error::BlixardResult,
    iroh_types::{
        GetTenantQuotaRequest, GetTenantQuotaResponse, GetTenantUsageRequest,
        GetTenantUsageResponse, ListTenantQuotasRequest, ListTenantQuotasResponse,
        RemoveTenantQuotaRequest, RemoveTenantQuotaResponse, SetTenantQuotaRequest,
        SetTenantQuotaResponse, TenantQuotaInfo, TenantUsageInfo,
    },
    node_shared::SharedNodeState,
    raft::{messages::RaftProposal, proposals::ProposalData},
    resource_quotas::{ResourceRequest, TenantQuota, TenantUsage},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Quota operation request types for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuotaOperationRequest {
    SetQuota(SetTenantQuotaRequest),
    GetQuota(GetTenantQuotaRequest),
    ListQuotas(ListTenantQuotasRequest),
    GetUsage(GetTenantUsageRequest),
    RemoveQuota(RemoveTenantQuotaRequest),
}

/// Quota operation response for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuotaOperationResponse {
    SetQuota(SetTenantQuotaResponse),
    GetQuota(GetTenantQuotaResponse),
    ListQuotas(ListTenantQuotasResponse),
    GetUsage(GetTenantUsageResponse),
    RemoveQuota(RemoveTenantQuotaResponse),
}

/// Quota service implementation
#[derive(Clone)]
pub struct QuotaServiceImpl {
    node: Arc<SharedNodeState>,
}

impl QuotaServiceImpl {
    /// Create a new quota service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }

    /// Set tenant quota
    pub async fn set_tenant_quota(
        &self,
        request: SetTenantQuotaRequest,
    ) -> BlixardResult<SetTenantQuotaResponse> {
        // Create TenantQuota from request
        let quota = TenantQuota {
            tenant_id: request.tenant_id.clone(),
            vm_limits: crate::resource_quotas::VmResourceLimits {
                max_vms: request.max_vms.unwrap_or(u32::MAX),
                max_vcpus: request.max_vcpus.unwrap_or(u32::MAX),
                max_memory_mb: request.max_memory_mb.unwrap_or(u64::MAX),
                max_disk_gb: request.max_disk_gb.unwrap_or(u64::MAX),
                max_vms_per_node: request.max_vms_per_node.unwrap_or(u32::MAX),
                overcommit_ratio: 1.0,
                priority: request.priority,
            },
            api_limits: crate::resource_quotas::ApiRateLimits::default(),
            storage_limits: crate::resource_quotas::StorageLimits::default(),
            enabled: true,
            created_at: std::time::SystemTime::now(),
            updated_at: std::time::SystemTime::now(),
        };

        // Send through Raft consensus
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: ProposalData::SetTenantQuota { quota },
            response_tx: None,
        };

        match self.node.send_raft_proposal(proposal).await {
            Ok(_) => Ok(SetTenantQuotaResponse {
                success: true,
                message: format!("Quota set for tenant '{}'", request.tenant_id),
            }),
            Err(e) => Ok(SetTenantQuotaResponse {
                success: false,
                message: format!("Failed to set quota: {}", e),
            }),
        }
    }

    /// Get tenant quota
    pub async fn get_tenant_quota(
        &self,
        request: GetTenantQuotaRequest,
    ) -> BlixardResult<GetTenantQuotaResponse> {
        if let Some(quota_manager) = self.node.get_quota_manager().await {
            let quota = quota_manager.get_tenant_quota(&request.tenant_id).await;
            Ok(GetTenantQuotaResponse {
                quota: Some(TenantQuotaInfo {
                    tenant_id: quota.tenant_id,
                    max_vms: quota.vm_limits.max_vms,
                    max_vcpus: quota.vm_limits.max_vcpus,
                    max_memory_mb: quota.vm_limits.max_memory_mb,
                    max_disk_gb: quota.vm_limits.max_disk_gb,
                    max_vms_per_node: quota.vm_limits.max_vms_per_node,
                    priority: quota.vm_limits.priority,
                }),
            })
        } else {
            Ok(GetTenantQuotaResponse { quota: None })
        }
    }

    /// List all tenant quotas
    pub async fn list_tenant_quotas(
        &self,
        _request: ListTenantQuotasRequest,
    ) -> BlixardResult<ListTenantQuotasResponse> {
        if let Some(quota_manager) = self.node.get_quota_manager().await {
            let quotas = quota_manager.get_all_quotas().await;
            let quota_infos = quotas
                .into_iter()
                .map(|(tenant_id, q)| TenantQuotaInfo {
                    tenant_id,
                    max_vms: q.vm_limits.max_vms,
                    max_vcpus: q.vm_limits.max_vcpus,
                    max_memory_mb: q.vm_limits.max_memory_mb,
                    max_disk_gb: q.vm_limits.max_disk_gb,
                    max_vms_per_node: q.vm_limits.max_vms_per_node,
                    priority: q.vm_limits.priority,
                })
                .collect();
            Ok(ListTenantQuotasResponse {
                quotas: quota_infos,
            })
        } else {
            Ok(ListTenantQuotasResponse { quotas: vec![] })
        }
    }

    /// Get tenant usage
    pub async fn get_tenant_usage(
        &self,
        request: GetTenantUsageRequest,
    ) -> BlixardResult<GetTenantUsageResponse> {
        if let Some(quota_manager) = self.node.get_quota_manager().await {
            let usage = quota_manager.get_tenant_usage(&request.tenant_id).await;
            Ok(GetTenantUsageResponse {
                usage: Some(TenantUsageInfo {
                    tenant_id: usage.tenant_id,
                    active_vms: usage.vm_usage.active_vms,
                    used_vcpus: usage.vm_usage.used_vcpus,
                    used_memory_mb: usage.vm_usage.used_memory_mb,
                    used_disk_gb: usage.vm_usage.used_disk_gb,
                    vms_per_node: usage.vm_usage.vms_per_node,
                }),
            })
        } else {
            Ok(GetTenantUsageResponse { usage: None })
        }
    }

    /// Remove tenant quota
    pub async fn remove_tenant_quota(
        &self,
        request: RemoveTenantQuotaRequest,
    ) -> BlixardResult<RemoveTenantQuotaResponse> {
        // Send through Raft consensus
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: ProposalData::RemoveTenantQuota {
                tenant_id: request.tenant_id.clone(),
            },
            response_tx: None,
        };

        match self.node.send_raft_proposal(proposal).await {
            Ok(_) => Ok(RemoveTenantQuotaResponse {
                success: true,
                message: format!("Quota removed for tenant '{}'", request.tenant_id),
            }),
            Err(e) => Ok(RemoveTenantQuotaResponse {
                success: false,
                message: format!("Failed to remove quota: {}", e),
            }),
        }
    }
}

/// Iroh protocol handler for quota service
pub struct QuotaProtocolHandler {
    service: QuotaServiceImpl,
}

impl QuotaProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: QuotaServiceImpl::new(node),
        }
    }

    /// Handle a quota operation request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        request: QuotaOperationRequest,
    ) -> BlixardResult<QuotaOperationResponse> {
        match request {
            QuotaOperationRequest::SetQuota(req) => {
                let response = self.service.set_tenant_quota(req).await?;
                Ok(QuotaOperationResponse::SetQuota(response))
            }
            QuotaOperationRequest::GetQuota(req) => {
                let response = self.service.get_tenant_quota(req).await?;
                Ok(QuotaOperationResponse::GetQuota(response))
            }
            QuotaOperationRequest::ListQuotas(req) => {
                let response = self.service.list_tenant_quotas(req).await?;
                Ok(QuotaOperationResponse::ListQuotas(response))
            }
            QuotaOperationRequest::GetUsage(req) => {
                let response = self.service.get_tenant_usage(req).await?;
                Ok(QuotaOperationResponse::GetUsage(response))
            }
            QuotaOperationRequest::RemoveQuota(req) => {
                let response = self.service.remove_tenant_quota(req).await?;
                Ok(QuotaOperationResponse::RemoveQuota(response))
            }
        }
    }
}