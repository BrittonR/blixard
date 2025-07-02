use blixard_core::types::{VmConfig, VmState, VmStatus};
use chrono::Utc;

fn main() {
    // Create a minimal VmConfig
    let config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/etc/blixard/vms/test-vm.yaml".to_string(),
        vcpus: 2,
        memory: 1024,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    };
    
    // Create a VmState
    let vm_state = VmState {
        name: config.name.clone(),
        config: config.clone(),
        status: VmStatus::Creating,
        node_id: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    // Try to serialize
    match bincode::serialize(&vm_state) {
        Ok(data) => {
            println!("Serialization successful! {} bytes", data.len());
            // Try to deserialize
            match bincode::deserialize::<VmState>(&data) {
                Ok(deserialized) => {
                    println!("Deserialization successful!");
                    println!("VM name: {}", deserialized.name);
                }
                Err(e) => {
                    println!("Deserialization failed: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("Serialization failed: {:?}", e);
            // Try individual fields
            println!("\nTrying individual fields:");
            
            // Try config alone
            match bincode::serialize(&config) {
                Ok(_) => println!("VmConfig: OK"),
                Err(e) => println!("VmConfig: FAILED - {:?}", e),
            }
            
            // Try anti_affinity
            match bincode::serialize(&config.anti_affinity) {
                Ok(_) => println!("anti_affinity: OK"),
                Err(e) => println!("anti_affinity: FAILED - {:?}", e),
            }
            
            // Try locality_preference
            match bincode::serialize(&config.locality_preference) {
                Ok(_) => println!("locality_preference: OK"),
                Err(e) => println!("locality_preference: FAILED - {:?}", e),
            }
            
            // Try health_check_config
            match bincode::serialize(&config.health_check_config) {
                Ok(_) => println!("health_check_config: OK"),
                Err(e) => println!("health_check_config: FAILED - {:?}", e),
            }
            
            // Try chrono fields
            let now = Utc::now();
            match bincode::serialize(&now) {
                Ok(_) => println!("chrono DateTime: OK"),
                Err(e) => println!("chrono DateTime: FAILED - {:?}", e),
            }
        }
    }
}