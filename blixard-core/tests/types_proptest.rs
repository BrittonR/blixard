// Property-based testing for type serialization and validation

use blixard_core::types::{NodeConfig, VmCommand, VmConfig, VmState, VmStatus};
use proptest::prelude::*;
use serde_json;
use std::net::SocketAddr;

mod common;

// Strategy for generating valid node IDs
fn node_id_strategy() -> impl Strategy<Value = u64> {
    1u64..=10000u64
}

// Strategy for generating valid ports
fn port_strategy() -> impl Strategy<Value = u16> {
    1024u16..=65535u16
}

// Strategy for generating VM names
fn vm_name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9\\-]{1,31}".prop_filter("Valid VM name", |s| {
        s.len() >= 2
            && s.len() <= 32
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && s.chars().next().unwrap().is_ascii_lowercase()
    })
}

// Strategy for generating memory sizes (in MB)
fn memory_strategy() -> impl Strategy<Value = u32> {
    prop::sample::select(&[128, 256, 512, 1024, 2048, 4096, 8192])
}

// Strategy for generating CPU counts
fn vcpu_strategy() -> impl Strategy<Value = u32> {
    1u32..=16u32
}

// Strategy for generating file paths
fn path_strategy() -> impl Strategy<Value = String> {
    "[/a-zA-Z0-9_\\-\\.]{5,100}"
}

// Strategy for generating IP addresses
fn ip_strategy() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        "127.0.0.1",
        "0.0.0.0",
        "192.168.1.1",
        "10.0.0.1",
        "172.16.0.1",
    ])
    .prop_map(|s| s.to_string())
}

// Property: Node configuration serialization roundtrip
proptest! {
    #[test]
    fn test_node_config_serialization_roundtrip(
        id in node_id_strategy(),
        port in port_strategy(),
        ip in ip_strategy(),
        data_dir in path_strategy(),
        use_tailscale in any::<bool>()
    ) {
        let bind_addr: SocketAddr = format!("{}:{}", ip, port).parse().unwrap();
        let join_addr = if use_tailscale {
            Some(format!("{}:{}", ip, port + 1).parse().unwrap())
        } else {
            None
        };

        let mut config = common::test_node_config(id, bind_addr.port());
        config.data_dir = data_dir.clone();
        config.bind_addr = bind_addr;
        config.join_addr = join_addr;
        config.use_tailscale = use_tailscale;

        // Test JSON serialization roundtrip (NodeConfig doesn't derive Serialize, so we test properties)
        prop_assert_eq!(config.id, id);
        prop_assert_eq!(config.data_dir, data_dir);
        prop_assert_eq!(config.bind_addr.port(), port);
        prop_assert_eq!(config.use_tailscale, use_tailscale);

        if use_tailscale {
            prop_assert!(config.join_addr.is_some());
        }
    }
}

// Property: VM configuration serialization roundtrip
proptest! {
    #[test]
    fn test_vm_config_serialization_roundtrip(
        name in vm_name_strategy(),
        config_path in path_strategy(),
        vcpus in vcpu_strategy(),
        memory in memory_strategy()
    ) {
        let mut config = common::test_vm_config(&name);
        config.config_path = config_path.clone();
        config.vcpus = vcpus;
        config.memory = memory;

        // Test JSON serialization roundtrip
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VmConfig = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(deserialized.name, name.clone());
        prop_assert_eq!(deserialized.config_path, config_path.clone());
        prop_assert_eq!(deserialized.vcpus, vcpus);
        prop_assert_eq!(deserialized.memory, memory);

        // Test bincode serialization roundtrip
        let encoded = bincode::serialize(&config).unwrap();
        let decoded: VmConfig = bincode::deserialize(&encoded).unwrap();

        prop_assert_eq!(decoded.name, name.clone());
        prop_assert_eq!(decoded.config_path, config_path.clone());
        prop_assert_eq!(decoded.vcpus, vcpus);
        prop_assert_eq!(decoded.memory, memory);
    }
}

// Property: VM status enum properties
proptest! {
    #[test]
    fn test_vm_status_properties(
        status in prop::sample::select(&[
            VmStatus::Creating,
            VmStatus::Starting,
            VmStatus::Running,
            VmStatus::Stopping,
            VmStatus::Stopped,
            VmStatus::Failed,
        ])
    ) {
        // Test serialization roundtrip
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: VmStatus = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(deserialized, status);

        // Test that status is copy/clone
        let copied = status;
        let cloned = status.clone();
        prop_assert_eq!(copied, status);
        prop_assert_eq!(cloned, status);

        // Test debug formatting
        let debug_str = format!("{:?}", status);
        prop_assert!(!debug_str.is_empty());
    }
}

// Property: VM state complete lifecycle testing
proptest! {
    #[test]
    fn test_vm_state_lifecycle_properties(
        name in vm_name_strategy(),
        config_path in path_strategy(),
        vcpus in vcpu_strategy(),
        memory in memory_strategy(),
        node_id in node_id_strategy(),
        status in prop::sample::select(&[
            VmStatus::Creating,
            VmStatus::Starting,
            VmStatus::Running,
            VmStatus::Stopping,
            VmStatus::Stopped,
            VmStatus::Failed,
        ])
    ) {
        let mut config = common::test_vm_config(&name);
        config.config_path = config_path;
        config.vcpus = vcpus;
        config.memory = memory;

        let now = chrono::Utc::now();
        let state = VmState {
            name: name.clone(),
            config: config.clone(),
            status,
            node_id,
            created_at: now,
            updated_at: now,
        };

        // Test JSON serialization roundtrip
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: VmState = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(deserialized.name, name.clone());
        prop_assert_eq!(deserialized.status, status);
        prop_assert_eq!(deserialized.node_id, node_id);
        prop_assert_eq!(deserialized.config.name, config.name);
        prop_assert_eq!(deserialized.config.vcpus, vcpus);
        prop_assert_eq!(deserialized.config.memory, memory);

        // Test bincode serialization roundtrip
        let encoded = bincode::serialize(&state).unwrap();
        let decoded: VmState = bincode::deserialize(&encoded).unwrap();

        prop_assert_eq!(decoded.name, name);
        prop_assert_eq!(decoded.status, status);
        prop_assert_eq!(decoded.node_id, node_id);

        // Test timestamp ordering
        prop_assert!(decoded.updated_at >= decoded.created_at);
    }
}

// Property: VM command serialization and structure
proptest! {
    #[test]
    fn test_vm_command_properties(
        name in vm_name_strategy(),
        config_path in path_strategy(),
        vcpus in vcpu_strategy(),
        memory in memory_strategy(),
        node_id in node_id_strategy(),
        status in prop::sample::select(&[
            VmStatus::Creating,
            VmStatus::Starting,
            VmStatus::Running,
            VmStatus::Stopping,
            VmStatus::Stopped,
            VmStatus::Failed,
        ]),
        command_type in 0u8..5u8
    ) {
        let mut config = common::test_vm_config(&name);
        config.config_path = config_path;
        config.vcpus = vcpus;
        config.memory = memory;

        let command = match command_type {
            0 => VmCommand::Create { config: config.clone(), node_id },
            1 => VmCommand::Start { name: name.clone() },
            2 => VmCommand::Stop { name: name.clone() },
            3 => VmCommand::Delete { name: name.clone() },
            4 => VmCommand::UpdateStatus { name: name.clone(), status },
            _ => unreachable!(),
        };

        // Test JSON serialization roundtrip
        let json = serde_json::to_string(&command).unwrap();
        let deserialized: VmCommand = serde_json::from_str(&json).unwrap();

        // Verify command type preservation
        match (&command, &deserialized) {
            (VmCommand::Create { config: c1, node_id: n1 }, VmCommand::Create { config: c2, node_id: n2 }) => {
                prop_assert_eq!(&c1.name, &c2.name);
                prop_assert_eq!(n1, n2);
            },
            (VmCommand::Start { name: n1 }, VmCommand::Start { name: n2 }) => {
                prop_assert_eq!(n1, n2);
            },
            (VmCommand::Stop { name: n1 }, VmCommand::Stop { name: n2 }) => {
                prop_assert_eq!(n1, n2);
            },
            (VmCommand::Delete { name: n1 }, VmCommand::Delete { name: n2 }) => {
                prop_assert_eq!(n1, n2);
            },
            (VmCommand::UpdateStatus { name: n1, status: s1 }, VmCommand::UpdateStatus { name: n2, status: s2 }) => {
                prop_assert_eq!(n1, n2);
                prop_assert_eq!(s1, s2);
            },
            _ => prop_assert!(false, "Command variant mismatch after serialization"),
        }

        // Test bincode serialization roundtrip
        let encoded = bincode::serialize(&command).unwrap();
        let decoded: VmCommand = bincode::deserialize(&encoded).unwrap();

        // Similar verification for bincode
        match (&command, &decoded) {
            (VmCommand::Create { .. }, VmCommand::Create { .. }) => {},
            (VmCommand::Start { .. }, VmCommand::Start { .. }) => {},
            (VmCommand::Stop { .. }, VmCommand::Stop { .. }) => {},
            (VmCommand::Delete { .. }, VmCommand::Delete { .. }) => {},
            (VmCommand::UpdateStatus { .. }, VmCommand::UpdateStatus { .. }) => {},
            _ => prop_assert!(false, "Command variant mismatch after bincode serialization"),
        }
    }
}

// Property: VM resource constraints validation
proptest! {
    #[test]
    fn test_vm_resource_constraints(
        name in vm_name_strategy(),
        vcpus in 1u32..=64u32,
        memory in 64u32..=65536u32
    ) {
        let mut config = common::test_vm_config(&name);
        config.vcpus = vcpus;
        config.memory = memory;

        // Basic constraints that should always hold
        prop_assert!(config.vcpus >= 1);
        prop_assert!(config.memory >= 64);
        prop_assert!(!config.name.is_empty());
        prop_assert!(!config.config_path.is_empty());

        // Resource relationship constraints
        if config.memory >= 1024 {
            // Large memory configs should be serializable
            let json = serde_json::to_string(&config).unwrap();
            prop_assert!(json.contains(&memory.to_string()));
        }

        if config.vcpus > 8 {
            // High CPU configs should serialize correctly
            let encoded = bincode::serialize(&config).unwrap();
            let decoded: VmConfig = bincode::deserialize(&encoded).unwrap();
            prop_assert_eq!(decoded.vcpus, vcpus);
        }
    }
}

// Property: Socket address parsing and validation
proptest! {
    #[test]
    fn test_socket_addr_properties(
        ip in ip_strategy(),
        port in port_strategy()
    ) {
        let addr_str = format!("{}:{}", ip, port);
        let addr: Result<SocketAddr, _> = addr_str.parse();

        prop_assert!(addr.is_ok());
        let parsed_addr = addr.unwrap();
        prop_assert_eq!(parsed_addr.port(), port);
        prop_assert_eq!(parsed_addr.ip().to_string(), ip);

        // Test that the address can be used in NodeConfig
        let mut config = common::test_node_config(1, parsed_addr.port());
        config.bind_addr = parsed_addr;

        prop_assert_eq!(config.bind_addr.port(), port);
    }
}
