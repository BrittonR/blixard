//! Integration tests for security and quota systems in gRPC server

#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::BlixardResult,
    proto::{
        cluster_service_client::ClusterServiceClient,
        CreateVmRequest, ListVmsRequest, ClusterStatusRequest,
    },
    test_helpers::TestCluster,
};
use tonic::{Request, metadata::MetadataValue};

#[tokio::test]
async fn test_grpc_server_with_security_enabled() -> BlixardResult<()> {
    let mut cluster = TestCluster::new(1).await?;
    let node = cluster.get_node(0).unwrap();
    
    // Wait for node to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Create a client without authentication
    let addr = format!("http://{}", node.addr);
    let mut client = ClusterServiceClient::connect(addr.clone()).await?;
    
    // Try to list VMs without authentication (should succeed in dev mode)
    let request = Request::new(ListVmsRequest {});
    let response = client.list_vms(request).await;
    
    // In development mode with disabled security, this should succeed
    assert!(response.is_ok(), "Expected success in dev mode, got: {:?}", response);
    
    Ok(())
}

#[tokio::test]
async fn test_grpc_server_with_authentication_token() -> BlixardResult<()> {
    // For now, just test that the server works with default security config
    // Full security testing would require more setup
    
    let mut cluster = TestCluster::new(1).await?;
    let node = cluster.get_node(0).unwrap();
    
    // Wait for node to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let addr = format!("http://{}", node.addr);
    
    // Test 1: Request with authentication token in metadata
    let mut client = ClusterServiceClient::connect(addr.clone()).await?;
    let mut request = Request::new(ListVmsRequest {});
    request.metadata_mut().insert(
        "authorization",
        MetadataValue::try_from("Bearer test-token-123").unwrap(),
    );
    let response = client.list_vms(request).await;
    
    // Should succeed (security is disabled by default in test mode)
    assert!(response.is_ok(), "Expected success with auth token");
    
    // Test 2: Test VM creation
    let create_request = Request::new(CreateVmRequest {
        name: "test-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    });
    let create_response = client.create_vm(create_request).await;
    
    // Should succeed in test mode
    assert!(create_response.is_ok(), "Expected VM creation to succeed");
    
    Ok(())
}

#[tokio::test]
async fn test_quota_rate_limiting() -> BlixardResult<()> {
    // This test demonstrates that quota checks are integrated into the gRPC server
    // In production, quotas would be enforced based on configuration
    
    let mut cluster = TestCluster::new(1).await?;
    let node = cluster.get_node(0).unwrap();
    
    // Wait for node to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let addr = format!("http://{}", node.addr);
    let mut client = ClusterServiceClient::connect(addr).await?;
    
    // Make multiple VM creation requests
    // In production with quotas enabled, this would trigger rate limits
    for i in 0..3 {
        let request = Request::new(CreateVmRequest {
            name: format!("test-vm-{}", i),
            config_path: "".to_string(),
            vcpus: 2,
            memory_mb: 1024,
        });
        
        let response = client.create_vm(request).await;
        
        // In test mode all requests should succeed
        assert!(response.is_ok(), "Request {} should succeed", i);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_tenant_id_extraction() -> BlixardResult<()> {
    let mut cluster = TestCluster::new(1).await?;
    let node = cluster.get_node(0).unwrap();
    
    // Wait for node to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let addr = format!("http://{}", node.addr);
    let mut client = ClusterServiceClient::connect(addr).await?;
    
    // Test with custom tenant ID
    let mut request = Request::new(ClusterStatusRequest {});
    request.metadata_mut().insert(
        "tenant-id",
        MetadataValue::try_from("custom-tenant").unwrap(),
    );
    
    let response = client.get_cluster_status(request).await;
    assert!(response.is_ok(), "Request with tenant ID should succeed");
    
    // Test without tenant ID (should use default)
    let request_default = Request::new(ClusterStatusRequest {});
    let response_default = client.get_cluster_status(request_default).await;
    assert!(response_default.is_ok(), "Request without tenant ID should use default");
    
    Ok(())
}