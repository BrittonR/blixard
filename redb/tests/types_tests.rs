// Type serialization and validation tests

use blixard::types::{NodeConfig, VmConfig, VmStatus, VmState, VmCommand};
use serde_json;
use std::net::SocketAddr;

mod common;

#[test]
fn test_node_config_creation() {
    let config = NodeConfig {
        id: 1,
        data_dir: "/tmp/test".to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: Some("127.0.0.1:7000".parse().unwrap()),
        use_tailscale: true,
    };
    
    assert_eq!(config.id, 1);
    assert_eq!(config.data_dir, "/tmp/test");
    assert!(config.use_tailscale);
    assert!(config.join_addr.is_some());
}

#[test]
fn test_vm_config_serialization() {
    let config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/path/to/config.nix".to_string(),
        vcpus: 2,
        memory: 1024,
    };
    
    // Test JSON serialization
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("test-vm"));
    assert!(json.contains("1024"));
    assert!(json.contains("config.nix"));
    
    // Test deserialization
    let deserialized: VmConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, config.name);
    assert_eq!(deserialized.memory, config.memory);
    assert_eq!(deserialized.vcpus, config.vcpus);
    assert_eq!(deserialized.config_path, config.config_path);
}

#[test]
fn test_vm_status_enum() {
    let statuses = vec![
        VmStatus::Creating,
        VmStatus::Starting,
        VmStatus::Running,
        VmStatus::Stopping,
        VmStatus::Stopped,
        VmStatus::Failed,
    ];
    
    for status in statuses {
        // Test serialization
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.is_empty());
        
        // Test deserialization
        let deserialized: VmStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
}

#[test]
fn test_vm_status_ordering() {
    // Test that status comparisons work
    assert_eq!(VmStatus::Running, VmStatus::Running);
    assert_ne!(VmStatus::Running, VmStatus::Stopped);
    
    // Test copy/clone
    let status = VmStatus::Running;
    let copied = status;
    let cloned = status.clone();
    
    assert_eq!(status, copied);
    assert_eq!(status, cloned);
}

#[test]
fn test_vm_state_full_lifecycle() {
    let config = VmConfig {
        name: "lifecycle-vm".to_string(),
        config_path: "/tmp/lifecycle.nix".to_string(),
        vcpus: 1,
        memory: 512,
    };
    
    let now = chrono::Utc::now();
    let state = VmState {
        name: "lifecycle-vm".to_string(),
        config: config.clone(),
        status: VmStatus::Creating,
        node_id: 42,
        created_at: now,
        updated_at: now,
    };
    
    // Test serialization
    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains("lifecycle-vm"));
    assert!(json.contains("Creating"));
    assert!(json.contains("42"));
    
    // Test deserialization
    let deserialized: VmState = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, state.name);
    assert_eq!(deserialized.status, state.status);
    assert_eq!(deserialized.node_id, state.node_id);
    assert_eq!(deserialized.config.name, config.name);
}

#[test]
fn test_vm_command_variants() {
    let config = VmConfig {
        name: "cmd-vm".to_string(),
        config_path: "/tmp/cmd.nix".to_string(),
        vcpus: 1,
        memory: 256,
    };
    
    let commands = vec![
        VmCommand::Create { config: config.clone(), node_id: 1 },
        VmCommand::Start { name: "test-vm".to_string() },
        VmCommand::Stop { name: "test-vm".to_string() },
        VmCommand::Delete { name: "test-vm".to_string() },
        VmCommand::UpdateStatus { name: "test-vm".to_string(), status: VmStatus::Running },
    ];
    
    for command in commands {
        // Test serialization
        let json = serde_json::to_string(&command).unwrap();
        assert!(!json.is_empty());
        
        // Test deserialization
        let deserialized: VmCommand = serde_json::from_str(&json).unwrap();
        
        // Verify command type matches
        match (&command, &deserialized) {
            (VmCommand::Create { .. }, VmCommand::Create { .. }) => {},
            (VmCommand::Start { .. }, VmCommand::Start { .. }) => {},
            (VmCommand::Stop { .. }, VmCommand::Stop { .. }) => {},
            (VmCommand::Delete { .. }, VmCommand::Delete { .. }) => {},
            (VmCommand::UpdateStatus { .. }, VmCommand::UpdateStatus { .. }) => {},
            _ => panic!("Command variant mismatch after serialization"),
        }
    }
}

#[test]
fn test_socket_addr_parsing() {
    let valid_addrs = vec![
        "127.0.0.1:7001",
        "0.0.0.0:8080",
        "192.168.1.100:9000",
        "[::1]:7001",
        "[2001:db8::1]:8080",
    ];
    
    for addr_str in valid_addrs {
        let addr: SocketAddr = addr_str.parse().unwrap();
        assert!(addr.port() > 0);
    }
}

#[test]
fn test_vm_config_validation_constraints() {
    // Test memory constraints
    let config = VmConfig {
        name: "mem-test".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 1,
        memory: 64, // Very low memory
    };
    
    assert!(config.memory > 0);
    
    // Test vcpu constraints
    let config = VmConfig {
        name: "cpu-test".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 0, // Invalid CPU count
        memory: 512,
    };
    
    // Should serialize even with invalid values (validation happens elsewhere)
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"vcpus\":0"));
}

#[test]
fn test_vm_name_edge_cases() {
    let edge_cases = vec![
        "", // Empty name
        "a", // Single character
        "very-long-vm-name-that-exceeds-normal-limits-and-keeps-going", // Very long name
        "vm with spaces", // Spaces
        "vm-with-special-chars-123", // Mixed characters
        "VM-UPPERCASE", // Uppercase
    ];
    
    for name in edge_cases {
        let config = VmConfig {
            name: name.to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory: 512,
        };
        
        // Should serialize regardless of validation
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains(&name));
    }
}

#[test]
fn test_timestamp_handling() {
    let now = chrono::Utc::now();
    let past = now - chrono::Duration::hours(1);
    let future = now + chrono::Duration::minutes(30);
    
    let state = VmState {
        name: "time-test".to_string(),
        config: VmConfig {
            name: "time-test".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory: 512,
        },
        status: VmStatus::Running,
        node_id: 1,
        created_at: past,
        updated_at: future,
    };
    
    // Test that times serialize and deserialize correctly
    let json = serde_json::to_string(&state).unwrap();
    let deserialized: VmState = serde_json::from_str(&json).unwrap();
    
    assert_eq!(deserialized.created_at, past);
    assert_eq!(deserialized.updated_at, future);
    assert!(deserialized.updated_at > deserialized.created_at);
}

#[test]
fn test_bincode_serialization() {
    let config = VmConfig {
        name: "bincode-test".to_string(),
        config_path: "/tmp/bincode.nix".to_string(),
        vcpus: 4,
        memory: 2048,
    };
    
    // Test bincode serialization (used for database storage)
    let encoded = bincode::serialize(&config).unwrap();
    assert!(!encoded.is_empty());
    
    let decoded: VmConfig = bincode::deserialize(&encoded).unwrap();
    assert_eq!(decoded.name, config.name);
    assert_eq!(decoded.memory, config.memory);
    assert_eq!(decoded.vcpus, config.vcpus);
}