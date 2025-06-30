//! Example showing Cedar integration with gRPC middleware

use blixard_core::cedar_authz::CedarAuthz;
use blixard_core::error::BlixardResult;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use tonic::{Request, Status};

/// Example auth context that would come from token validation
#[derive(Clone)]
struct AuthContext {
    user_id: String,
    tenant_id: String,
    roles: Vec<String>,
}

/// Example middleware that uses Cedar for authorization
struct CedarMiddleware {
    cedar: CedarAuthz,
}

impl CedarMiddleware {
    async fn new() -> BlixardResult<Self> {
        let schema_path = PathBuf::from("cedar/schema.cedarschema.json");
        let policies_dir = PathBuf::from("cedar/policies");
        let cedar = CedarAuthz::new(&schema_path, &policies_dir).await?;
        
        Ok(Self { cedar })
    }
    
    /// Authenticate and authorize using Cedar
    async fn authenticate_and_authorize_cedar<T>(
        &self,
        request: Request<T>,
        action: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<(Request<T>, AuthContext), Status> {
        // In a real implementation, this would extract and validate the auth token
        let auth_context = self.extract_auth_context(&request)?;
        
        // Build the full resource identifier
        let resource = format!("{}::\"{}\"", resource_type, resource_id);
        
        // Build Cedar context with current time and other contextual data
        let now = chrono::Local::now();
        let mut context = HashMap::new();
        context.insert("hour".to_string(), json!(now.hour()));
        context.insert("day_of_week".to_string(), json!(now.weekday().to_string()));
        
        // Check authorization with Cedar
        let principal = format!("User::\"{}\"", auth_context.user_id);
        let action_str = format!("Action::\"{}\"", action);
        
        let authorized = self.cedar
            .is_authorized(&principal, &action_str, &resource, context)
            .await
            .map_err(|e| Status::internal(format!("Authorization error: {}", e)))?;
        
        if !authorized {
            return Err(Status::permission_denied(format!(
                "User {} is not authorized to {} on {}",
                auth_context.user_id, action, resource
            )));
        }
        
        Ok((request, auth_context))
    }
    
    /// Extract auth context from request (simplified)
    fn extract_auth_context<T>(&self, _request: &Request<T>) -> Result<AuthContext, Status> {
        // In a real implementation, this would:
        // 1. Extract the Authorization header
        // 2. Validate the token
        // 3. Extract user information from the token
        
        // For this example, we'll return a hardcoded context
        Ok(AuthContext {
            user_id: "operator-123".to_string(),
            tenant_id: "acme".to_string(),
            roles: vec!["operator".to_string()],
        })
    }
}

/// Example VM service implementation using Cedar
struct VmService {
    middleware: CedarMiddleware,
}

impl VmService {
    async fn create_vm(&self, request: Request<CreateVmRequest>) -> Result<String, Status> {
        // Authorize the request using Cedar
        let (request, auth_context) = self.middleware
            .authenticate_and_authorize_cedar(
                request,
                "createVM",
                "Tenant",
                &auth_context.tenant_id,
            )
            .await?;
        
        let req = request.into_inner();
        println!("✅ User {} authorized to create VM: {}", auth_context.user_id, req.name);
        
        // Additional authorization check for node capacity
        if let Some(node_id) = req.node_id {
            let mut context = HashMap::new();
            context.insert("requested_cpu".to_string(), json!(req.cpu));
            context.insert("requested_memory".to_string(), json!(req.memory));
            
            let principal = format!("User::\"{}\"", auth_context.user_id);
            let resource = format!("Node::\"{}\"", node_id);
            
            let authorized = self.middleware.cedar
                .is_authorized(&principal, "Action::\"createVM\"", &resource, context)
                .await
                .map_err(|e| Status::internal(format!("Node capacity check failed: {}", e)))?;
            
            if !authorized {
                return Err(Status::resource_exhausted(
                    "Node does not have sufficient capacity for the requested VM"
                ));
            }
        }
        
        // Simulate VM creation
        Ok(format!("vm-{}", uuid::Uuid::new_v4()))
    }
    
    async fn delete_vm(&self, request: Request<DeleteVmRequest>) -> Result<(), Status> {
        let vm_id = request.get_ref().vm_id.clone();
        
        // First, fetch VM details to check priority and state
        // In a real system, this would query the database
        let vm_priority = 100; // Example priority
        let vm_state = "Running";
        
        // Build context for policy evaluation
        let now = chrono::Local::now();
        let mut context = HashMap::new();
        context.insert("hour".to_string(), json!(now.hour()));
        context.insert("vm_priority".to_string(), json!(vm_priority));
        context.insert("vm_state".to_string(), json!(vm_state));
        
        // Authorize with context
        let (_request, auth_context) = self.middleware
            .authenticate_and_authorize_cedar(
                request,
                "deleteVM",
                "VM",
                &vm_id,
            )
            .await?;
        
        println!("✅ User {} authorized to delete VM: {}", auth_context.user_id, vm_id);
        
        // Simulate VM deletion
        Ok(())
    }
}

// Request/Response types
#[derive(Debug)]
struct CreateVmRequest {
    name: String,
    cpu: i64,
    memory: i64,
    node_id: Option<String>,
}

#[derive(Debug)]
struct DeleteVmRequest {
    vm_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Cedar gRPC Integration Example\n");
    
    // Initialize Cedar middleware
    let middleware = CedarMiddleware::new().await?;
    
    // Setup test entities
    setup_test_data(&middleware.cedar).await?;
    
    // Create service instance
    let service = VmService { middleware };
    
    // Test scenarios
    println!("📋 Testing VM operations with Cedar authorization...\n");
    
    // Scenario 1: Create VM (should succeed)
    println!("1️⃣ Testing VM creation:");
    let create_req = Request::new(CreateVmRequest {
        name: "web-server-1".to_string(),
        cpu: 4,
        memory: 8192,
        node_id: Some("node1".to_string()),
    });
    
    match service.create_vm(create_req).await {
        Ok(vm_id) => println!("   ✅ VM created successfully: {}", vm_id),
        Err(e) => println!("   ❌ Failed to create VM: {}", e),
    }
    
    // Scenario 2: Delete VM during business hours (should fail for operator)
    println!("\n2️⃣ Testing VM deletion during business hours:");
    let delete_req = Request::new(DeleteVmRequest {
        vm_id: "vm-123".to_string(),
    });
    
    let now = chrono::Local::now();
    if now.hour() >= 9 && now.hour() < 17 {
        match service.delete_vm(delete_req).await {
            Ok(_) => println!("   ✅ VM deleted successfully"),
            Err(e) => println!("   ❌ Expected failure: {}", e),
        }
    } else {
        println!("   ⏭️  Skipping (not business hours)");
    }
    
    // Scenario 3: Create VM exceeding node capacity
    println!("\n3️⃣ Testing VM creation with insufficient node capacity:");
    let create_req = Request::new(CreateVmRequest {
        name: "large-vm".to_string(),
        cpu: 100, // Exceeds node capacity
        memory: 100000,
        node_id: Some("node1".to_string()),
    });
    
    match service.create_vm(create_req).await {
        Ok(vm_id) => println!("   ✅ VM created (unexpected): {}", vm_id),
        Err(e) => println!("   ❌ Expected failure: {}", e),
    }
    
    println!("\n✨ Demo complete!");
    
    Ok(())
}

/// Setup test data for the example
async fn setup_test_data(cedar: &CedarAuthz) -> BlixardResult<()> {
    // Add roles
    cedar.add_entity("Role", "operator", HashMap::new(), vec![]).await?;
    
    // Add tenant
    let mut tenant_attrs = HashMap::new();
    tenant_attrs.insert("name".to_string(), json!("acme"));
    tenant_attrs.insert("quota_vms".to_string(), json!(50));
    tenant_attrs.insert("quota_cpu".to_string(), json!(1000));
    tenant_attrs.insert("quota_memory".to_string(), json!(8192));
    
    cedar.add_entity("Tenant", "acme", tenant_attrs, vec![]).await?;
    
    // Add node
    let mut node_attrs = HashMap::new();
    node_attrs.insert("available_cpu".to_string(), json!(16));
    node_attrs.insert("available_memory".to_string(), json!(32768));
    
    cedar.add_entity("Node", "node1", node_attrs, vec![]).await?;
    
    // Add user
    let mut user_attrs = HashMap::new();
    user_attrs.insert("email".to_string(), json!("operator@acme.com"));
    user_attrs.insert("tenant_id".to_string(), json!("acme"));
    
    cedar.add_entity(
        "User",
        "operator-123",
        user_attrs,
        vec!["Role::\"operator\"".to_string(), "Tenant::\"acme\"".to_string()],
    ).await?;
    
    // Add VM for deletion test
    let mut vm_attrs = HashMap::new();
    vm_attrs.insert("name".to_string(), json!("test-vm"));
    vm_attrs.insert("tenant_id".to_string(), json!("acme"));
    vm_attrs.insert("priority".to_string(), json!(100));
    vm_attrs.insert("state".to_string(), json!("Running"));
    
    cedar.add_entity(
        "VM",
        "vm-123",
        vm_attrs,
        vec!["Tenant::\"acme\"".to_string()],
    ).await?;
    
    Ok(())
}