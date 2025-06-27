//! Network client abstractions for testability
//!
//! This module provides trait-based abstractions for network operations,
//! enabling testing without actual network dependencies.

use async_trait::async_trait;
use std::time::Duration;
use crate::{
    error::BlixardResult,
    proto::{
        cluster_service_client::ClusterServiceClient,
        blixard_service_client::BlixardServiceClient,
        JoinRequest, JoinResponse,
        ClusterStatusRequest, ClusterStatusResponse,
        HealthCheckRequest, HealthCheckResponse,
    },
};
use tonic::{transport::Channel, Request, Response};

/// Abstraction for network client operations
#[async_trait]
pub trait NetworkClient: Send + Sync {
    /// Create a cluster service client for a given address
    async fn create_cluster_client(&self, address: &str) -> BlixardResult<Box<dyn ClusterClient>>;
    
    /// Create a blixard service client for a given address
    async fn create_blixard_client(&self, address: &str) -> BlixardResult<Box<dyn BlixardClient>>;
    
    /// Check if an address is reachable
    async fn is_reachable(&self, address: &str, timeout: Duration) -> BlixardResult<bool>;
}

/// Abstraction for cluster service operations
#[async_trait]
pub trait ClusterClient: Send + Sync {
    /// Join cluster operation
    async fn join_cluster(&mut self, request: JoinRequest) -> BlixardResult<JoinResponse>;
    
    /// Get cluster status
    async fn get_cluster_status(&mut self, request: ClusterStatusRequest) -> BlixardResult<ClusterStatusResponse>;
}

/// Abstraction for blixard service operations
#[async_trait]
pub trait BlixardClient: Send + Sync {
    /// Health check operation
    async fn health_check(&mut self, request: HealthCheckRequest) -> BlixardResult<HealthCheckResponse>;
}

// Production implementations

/// Production network client
pub struct TonicNetworkClient;

impl TonicNetworkClient {
    /// Create new instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for TonicNetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkClient for TonicNetworkClient {
    async fn create_cluster_client(&self, address: &str) -> BlixardResult<Box<dyn ClusterClient>> {
        let channel = Channel::from_shared(format!("http://{}", address))
            .map_err(|e| crate::error::BlixardError::NetworkError(
                tonic::transport::Error::from(e)
            ))?
            .connect()
            .await?;
            
        Ok(Box::new(TonicClusterClient {
            inner: ClusterServiceClient::new(channel),
        }))
    }
    
    async fn create_blixard_client(&self, address: &str) -> BlixardResult<Box<dyn BlixardClient>> {
        let channel = Channel::from_shared(format!("http://{}", address))
            .map_err(|e| crate::error::BlixardError::NetworkError(
                tonic::transport::Error::from(e)
            ))?
            .connect()
            .await?;
            
        Ok(Box::new(TonicBlixardClient {
            inner: BlixardServiceClient::new(channel),
        }))
    }
    
    async fn is_reachable(&self, address: &str, timeout: Duration) -> BlixardResult<bool> {
        match tokio::time::timeout(
            timeout,
            Channel::from_shared(format!("http://{}", address))
                .map_err(|e| crate::error::BlixardError::NetworkError(
                    tonic::transport::Error::from(e)
                ))?
                .connect()
        ).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(_)) => Ok(false),
            Err(_) => Ok(false), // Timeout
        }
    }
}

/// Wrapper for tonic cluster client
struct TonicClusterClient {
    inner: ClusterServiceClient<Channel>,
}

#[async_trait]
impl ClusterClient for TonicClusterClient {
    async fn join_cluster(&mut self, request: JoinRequest) -> BlixardResult<JoinResponse> {
        let response = self.inner.join_cluster(Request::new(request)).await?;
        Ok(response.into_inner())
    }
    
    async fn get_cluster_status(&mut self, request: ClusterStatusRequest) -> BlixardResult<ClusterStatusResponse> {
        let response = self.inner.get_cluster_status(Request::new(request)).await?;
        Ok(response.into_inner())
    }
}

/// Wrapper for tonic blixard client
struct TonicBlixardClient {
    inner: BlixardServiceClient<Channel>,
}

#[async_trait]
impl BlixardClient for TonicBlixardClient {
    async fn health_check(&mut self, request: HealthCheckRequest) -> BlixardResult<HealthCheckResponse> {
        let response = self.inner.health_check(Request::new(request)).await?;
        Ok(response.into_inner())
    }
}

// Mock implementations

use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// Mock network client for testing
pub struct MockNetworkClient {
    reachable_addresses: Arc<RwLock<HashMap<String, bool>>>,
    cluster_responses: Arc<RwLock<HashMap<String, MockClusterResponses>>>,
    health_responses: Arc<RwLock<HashMap<String, HealthCheckResponse>>>,
}

#[derive(Clone, Default)]
struct MockClusterResponses {
    join_response: Option<JoinResponse>,
    status_response: Option<ClusterStatusResponse>,
}

impl MockNetworkClient {
    /// Create new mock client
    pub fn new() -> Self {
        Self {
            reachable_addresses: Arc::new(RwLock::new(HashMap::new())),
            cluster_responses: Arc::new(RwLock::new(HashMap::new())),
            health_responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Set whether an address is reachable
    pub async fn set_reachable(&self, address: &str, reachable: bool) {
        self.reachable_addresses.write().await.insert(address.to_string(), reachable);
    }
    
    /// Set join cluster response for an address
    pub async fn set_join_response(&self, address: &str, response: JoinResponse) {
        let mut responses = self.cluster_responses.write().await;
        let entry = responses.entry(address.to_string()).or_default();
        entry.join_response = Some(response);
    }
    
    /// Set cluster status response for an address
    pub async fn set_status_response(&self, address: &str, response: ClusterStatusResponse) {
        let mut responses = self.cluster_responses.write().await;
        let entry = responses.entry(address.to_string()).or_default();
        entry.status_response = Some(response);
    }
    
    /// Set health check response for an address
    pub async fn set_health_response(&self, address: &str, response: HealthCheckResponse) {
        self.health_responses.write().await.insert(address.to_string(), response);
    }
}

impl Default for MockNetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkClient for MockNetworkClient {
    async fn create_cluster_client(&self, address: &str) -> BlixardResult<Box<dyn ClusterClient>> {
        let responses = self.cluster_responses.read().await
            .get(address)
            .cloned()
            .unwrap_or_default();
            
        Ok(Box::new(MockClusterClient { responses }))
    }
    
    async fn create_blixard_client(&self, address: &str) -> BlixardResult<Box<dyn BlixardClient>> {
        let response = self.health_responses.read().await
            .get(address)
            .cloned();
            
        Ok(Box::new(MockBlixardClient { response }))
    }
    
    async fn is_reachable(&self, address: &str, _timeout: Duration) -> BlixardResult<bool> {
        Ok(self.reachable_addresses.read().await
            .get(address)
            .copied()
            .unwrap_or(false))
    }
}

/// Mock cluster client
struct MockClusterClient {
    responses: MockClusterResponses,
}

#[async_trait]
impl ClusterClient for MockClusterClient {
    async fn join_cluster(&mut self, _request: JoinRequest) -> BlixardResult<JoinResponse> {
        self.responses.join_response.clone()
            .ok_or_else(|| crate::error::BlixardError::Internal {
                message: "No mock join response configured".to_string()
            })
    }
    
    async fn get_cluster_status(&mut self, _request: ClusterStatusRequest) -> BlixardResult<ClusterStatusResponse> {
        self.responses.status_response.clone()
            .ok_or_else(|| crate::error::BlixardError::Internal {
                message: "No mock status response configured".to_string()
            })
    }
}

/// Mock blixard client
struct MockBlixardClient {
    response: Option<HealthCheckResponse>,
}

#[async_trait]
impl BlixardClient for MockBlixardClient {
    async fn health_check(&mut self, _request: HealthCheckRequest) -> BlixardResult<HealthCheckResponse> {
        self.response.clone()
            .ok_or_else(|| crate::error::BlixardError::Internal {
                message: "No mock health response configured".to_string()
            })
    }
}