//! Example of using Blixard gRPC client with authentication

use blixard_core::{
    error::BlixardResult,
    proto::{
        cluster_service_client::ClusterServiceClient,
        ListVmsRequest, CreateVmRequest,
    },
};
use tonic::{Request, metadata::MetadataValue};

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=info")
        .init();

    // Connect to the gRPC server
    let addr = "http://127.0.0.1:7001";
    let mut client = ClusterServiceClient::connect(addr).await
        .expect("Failed to connect to gRPC server");

    println!("Connected to Blixard gRPC server at {}", addr);

    // Example 1: Make a request without authentication
    println!("\n1. Request without authentication:");
    let request = Request::new(ListVmsRequest {});
    match client.list_vms(request).await {
        Ok(response) => {
            let vms = response.into_inner().vms;
            println!("  Success! Found {} VMs", vms.len());
            for vm in vms {
                println!("    - {} ({})", vm.name, vm.state);
            }
        }
        Err(e) => {
            println!("  Failed: {}", e);
        }
    }

    // Example 2: Make a request with Bearer token authentication
    println!("\n2. Request with Bearer token authentication:");
    let mut request_with_auth = Request::new(ListVmsRequest {});
    request_with_auth.metadata_mut().insert(
        "authorization",
        MetadataValue::from_str("Bearer my-api-token-123").unwrap(),
    );
    
    match client.list_vms(request_with_auth).await {
        Ok(response) => {
            let vms = response.into_inner().vms;
            println!("  Success! Found {} VMs", vms.len());
        }
        Err(e) => {
            println!("  Failed: {}", e);
        }
    }

    // Example 3: Make a request with custom tenant ID
    println!("\n3. Request with custom tenant ID:");
    let mut request_with_tenant = Request::new(CreateVmRequest {
        name: "example-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    });
    
    // Set both authentication and tenant ID
    request_with_tenant.metadata_mut().insert(
        "authorization",
        MetadataValue::from_str("Bearer my-api-token-123").unwrap(),
    );
    request_with_tenant.metadata_mut().insert(
        "tenant-id",
        MetadataValue::from_str("customer-123").unwrap(),
    );
    
    match client.create_vm(request_with_tenant).await {
        Ok(response) => {
            let resp = response.into_inner();
            if resp.success {
                println!("  Success! Created VM: {}", resp.vm_id);
            } else {
                println!("  Failed to create VM: {}", resp.message);
            }
        }
        Err(e) => {
            println!("  Failed: {}", e);
            if e.code() == tonic::Code::ResourceExhausted {
                println!("  (This might be due to quota limits for the tenant)");
            }
        }
    }

    // Example 4: Demonstrate rate limiting
    println!("\n4. Testing rate limiting (making multiple requests):");
    for i in 0..5 {
        let request = Request::new(ListVmsRequest {});
        match client.list_vms(request).await {
            Ok(_) => println!("  Request {} succeeded", i + 1),
            Err(e) => {
                println!("  Request {} failed: {}", i + 1, e);
                if e.code() == tonic::Code::ResourceExhausted {
                    println!("  (Rate limit exceeded)");
                }
            }
        }
        
        // Small delay between requests
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\nNote: In production, you would need to:");
    println!("  1. Configure security settings in blixard.toml");
    println!("  2. Set up proper authentication tokens");
    println!("  3. Configure quota limits for tenants");
    println!("  4. Enable TLS for secure communication");

    Ok(())
}