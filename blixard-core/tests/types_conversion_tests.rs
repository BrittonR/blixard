// Comprehensive tests for From/Into trait implementations

use blixard_core::types::{NodeId, NodeState, OptionExt, SocketAddrExt, VmId, VmStatus};
use blixard_core::error::BlixardError;
use std::net::SocketAddr;
use uuid::Uuid;

mod common;

#[test]
fn test_vm_id_conversions() {
    // Test UUID -> VmId conversion
    let uuid = Uuid::new_v4();
    let vm_id: VmId = uuid.into();
    assert_eq!(vm_id.0, uuid);

    // Test VmId -> UUID conversion
    let back_to_uuid: Uuid = vm_id.into();
    assert_eq!(back_to_uuid, uuid);

    // Test VmId -> String conversion
    let id_string: String = vm_id.into();
    assert_eq!(id_string, uuid.to_string());

    // Test Display trait
    assert_eq!(vm_id.to_string(), uuid.to_string());
}

#[test]
fn test_vm_id_string_parsing() {
    let uuid = Uuid::new_v4();
    let uuid_str = uuid.to_string();

    // Test FromStr implementation
    let vm_id: VmId = uuid_str.parse().unwrap();
    assert_eq!(vm_id.0, uuid);

    // Test TryFrom<String>
    let vm_id2: VmId = uuid_str.clone().try_into().unwrap();
    assert_eq!(vm_id2.0, uuid);

    // Test TryFrom<&str>
    let vm_id3: VmId = uuid_str.as_str().try_into().unwrap();
    assert_eq!(vm_id3.0, uuid);

    // Test invalid UUID string
    let invalid = "not-a-uuid";
    assert!(invalid.parse::<VmId>().is_err());
}

#[test]
fn test_vm_id_from_string_deterministic() {
    let test_name = "test-vm";
    let vm_id1 = VmId::from_string(test_name);
    let vm_id2 = VmId::from_string(test_name);
    
    // Should be deterministic
    assert_eq!(vm_id1, vm_id2);
    
    // Different names should produce different IDs
    let vm_id3 = VmId::from_string("different-vm");
    assert_ne!(vm_id1, vm_id3);
}

#[test]
fn test_vm_status_conversions() {
    let test_cases = vec![
        (VmStatus::Creating, "creating", 1),
        (VmStatus::Starting, "starting", 2),
        (VmStatus::Running, "running", 3),
        (VmStatus::Stopping, "stopping", 4),
        (VmStatus::Stopped, "stopped", 5),
        (VmStatus::Failed, "failed", 6),
    ];

    for (status, expected_str, expected_i32) in test_cases {
        // Test Display trait
        assert_eq!(status.to_string(), expected_str);

        // Test Into<String>
        let status_string: String = status.into();
        assert_eq!(status_string, expected_str);

        // Test Into<i32>
        let status_code: i32 = status.into();
        assert_eq!(status_code, expected_i32);

        // Test FromStr
        let parsed_status: VmStatus = expected_str.parse().unwrap();
        assert_eq!(parsed_status, status);

        // Test TryFrom<String>
        let from_string: VmStatus = expected_str.to_string().try_into().unwrap();
        assert_eq!(from_string, status);

        // Test TryFrom<&str>
        let from_str: VmStatus = expected_str.try_into().unwrap();
        assert_eq!(from_str, status);

        // Test TryFrom<i32>
        let from_i32: VmStatus = expected_i32.try_into().unwrap();
        assert_eq!(from_i32, status);
    }
}

#[test]
fn test_vm_status_case_insensitive() {
    let test_cases = vec![
        ("RUNNING", VmStatus::Running),
        ("Running", VmStatus::Running),
        ("RuNnInG", VmStatus::Running),
        ("STOPPED", VmStatus::Stopped),
        ("failed", VmStatus::Failed),
    ];

    for (input, expected) in test_cases {
        let parsed: VmStatus = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }
}

#[test]
fn test_vm_status_invalid_inputs() {
    // Test invalid string
    let result = "invalid_status".parse::<VmStatus>();
    assert!(result.is_err());
    if let Err(BlixardError::InvalidInput { field, message }) = result {
        assert_eq!(field, "vm_status");
        assert!(message.contains("Invalid VM status"));
    }

    // Test invalid i32
    let result = VmStatus::try_from(999);
    assert!(result.is_err());
    if let Err(BlixardError::InvalidInput { field, message }) = result {
        assert_eq!(field, "vm_status");
        assert!(message.contains("Invalid VM status code"));
    }
}

#[test]
fn test_node_state_conversions() {
    let test_cases = vec![
        (NodeState::Uninitialized, "uninitialized"),
        (NodeState::Initialized, "initialized"),
        (NodeState::JoiningCluster, "joining_cluster"),
        (NodeState::Active, "active"),
        (NodeState::LeavingCluster, "leaving_cluster"),
        (NodeState::Error, "error"),
    ];

    for (state, expected_str) in test_cases {
        // Test Display trait
        assert_eq!(state.to_string(), expected_str);

        // Test Into<String>
        let state_string: String = state.into();
        assert_eq!(state_string, expected_str);

        // Test FromStr
        let parsed_state: NodeState = expected_str.parse().unwrap();
        assert_eq!(parsed_state, state);

        // Test TryFrom<String>
        let from_string: NodeState = expected_str.to_string().try_into().unwrap();
        assert_eq!(from_string, state);

        // Test TryFrom<&str>
        let from_str: NodeState = expected_str.try_into().unwrap();
        assert_eq!(from_str, state);
    }
}

#[test]
fn test_node_state_hyphen_variants() {
    // Test that both underscore and hyphen variants work
    let joining_underscore: NodeState = "joining_cluster".parse().unwrap();
    let joining_hyphen: NodeState = "joining-cluster".parse().unwrap();
    assert_eq!(joining_underscore, NodeState::JoiningCluster);
    assert_eq!(joining_hyphen, NodeState::JoiningCluster);

    let leaving_underscore: NodeState = "leaving_cluster".parse().unwrap();
    let leaving_hyphen: NodeState = "leaving-cluster".parse().unwrap();
    assert_eq!(leaving_underscore, NodeState::LeavingCluster);
    assert_eq!(leaving_hyphen, NodeState::LeavingCluster);
}

#[test]
fn test_node_id_conversions() {
    let id_value = 12345u64;
    
    // Test From<u64>
    let node_id: NodeId = id_value.into();
    assert_eq!(node_id.as_u64(), id_value);

    // Test Into<u64>
    let back_to_u64: u64 = node_id.into();
    assert_eq!(back_to_u64, id_value);

    // Test Into<String>
    let id_string: String = node_id.into();
    assert_eq!(id_string, "12345");

    // Test Display trait
    assert_eq!(node_id.to_string(), "12345");

    // Test new() constructor
    let node_id2 = NodeId::new(id_value);
    assert_eq!(node_id2, node_id);
}

#[test]
fn test_node_id_string_parsing() {
    let id_str = "54321";
    
    // Test FromStr
    let node_id: NodeId = id_str.parse().unwrap();
    assert_eq!(node_id.as_u64(), 54321);

    // Test TryFrom<String>
    let node_id2: NodeId = id_str.to_string().try_into().unwrap();
    assert_eq!(node_id2.as_u64(), 54321);

    // Test TryFrom<&str>
    let node_id3: NodeId = id_str.try_into().unwrap();
    assert_eq!(node_id3.as_u64(), 54321);

    // Test invalid number string
    let result = "not-a-number".parse::<NodeId>();
    assert!(result.is_err());
    if let Err(BlixardError::InvalidInput { field, message }) = result {
        assert_eq!(field, "node_id");
        assert!(message.contains("Invalid node ID"));
    }
}

#[test]
fn test_socket_addr_ext() {
    // Test valid addresses
    let valid_addresses = vec![
        "127.0.0.1:8080",
        "0.0.0.0:3000",
        "192.168.1.100:9000",
        "[::1]:7001",
        "[2001:db8::1]:8080",
    ];

    for addr_str in valid_addresses {
        // Test try_from_str
        let addr1 = SocketAddr::try_from_str(addr_str).unwrap();
        assert!(addr1.port() > 0);

        // Test try_from_string
        let addr2 = SocketAddr::try_from_string(addr_str.to_string()).unwrap();
        assert_eq!(addr1, addr2);
    }

    // Test invalid address
    let result = SocketAddr::try_from_str("invalid-address");
    assert!(result.is_err());
    if let Err(BlixardError::InvalidInput { field, message }) = result {
        assert_eq!(field, "socket_address");
        assert!(message.contains("Invalid socket address"));
    }
}

#[test]
fn test_metadata_conversions() {
    use blixard_core::types::metadata;

    // Test from_string_pairs
    let pairs = vec![
        ("key1".to_string(), "value1".to_string()),
        ("key2".to_string(), "value2".to_string()),
    ];
    let map = metadata::from_string_pairs(pairs);
    assert_eq!(map.get("key1"), Some(&"value1".to_string()));
    assert_eq!(map.get("key2"), Some(&"value2".to_string()));

    // Test from_str_pairs
    let str_pairs = vec![("key3", "value3"), ("key4", "value4")];
    let map2 = metadata::from_str_pairs(str_pairs);
    assert_eq!(map2.get("key3"), Some(&"value3".to_string()));
    assert_eq!(map2.get("key4"), Some(&"value4".to_string()));

    // Test from_pairs with mixed types
    let mixed_pairs = vec![("key5", "value5"), ("key6", "value6")];
    let map3 = metadata::from_pairs(mixed_pairs);
    assert_eq!(map3.get("key5"), Some(&"value5".to_string()));
    assert_eq!(map3.get("key6"), Some(&"value6".to_string()));
}

#[test]
fn test_option_ext_helpers() {
    // Test ok_or_invalid_input
    let some_value = Some("test");
    let result = some_value.ok_or_invalid_input("test_field", "test message");
    assert_eq!(result.unwrap(), "test");

    let none_value: Option<&str> = None;
    let result = none_value.ok_or_invalid_input("test_field", "test message");
    assert!(result.is_err());
    if let Err(BlixardError::InvalidInput { field, message }) = result {
        assert_eq!(field, "test_field");
        assert_eq!(message, "test message");
    }

    // Test ok_or_not_found
    let some_value = Some(42);
    let result = some_value.ok_or_not_found("test_resource");
    assert_eq!(result.unwrap(), 42);

    let none_value: Option<i32> = None;
    let result = none_value.ok_or_not_found("test_resource");
    assert!(result.is_err());
    if let Err(BlixardError::NotFound { resource }) = result {
        assert_eq!(resource, "test_resource");
    }
}

#[test]
fn test_error_helper_methods() {
    // Test BlixardError helper methods
    let error = BlixardError::not_found("test_vm");
    if let BlixardError::NotFound { resource } = error {
        assert_eq!(resource, "test_vm");
    }

    let error = BlixardError::already_exists("test_vm");
    if let BlixardError::AlreadyExists { resource } = error {
        assert_eq!(resource, "test_vm");
    }

    let error = BlixardError::invalid_input("vm_name", "Name cannot be empty");
    if let BlixardError::InvalidInput { field, message } = error {
        assert_eq!(field, "vm_name");
        assert_eq!(message, "Name cannot be empty");
    }

    let error = BlixardError::validation_error("memory", "Must be at least 512MB");
    if let BlixardError::Validation { field, message } = error {
        assert_eq!(field, "memory");
        assert_eq!(message, "Must be at least 512MB");
    }

    let error = BlixardError::resource_exhausted("cpu_cores");
    if let BlixardError::ResourceExhausted { resource } = error {
        assert_eq!(resource, "cpu_cores");
    }

    let timeout = std::time::Duration::from_secs(30);
    let error = BlixardError::timeout_error("vm_start", timeout);
    if let BlixardError::Timeout { operation, duration } = error {
        assert_eq!(operation, "vm_start");
        assert_eq!(duration, timeout);
    }

    let error = BlixardError::connection_error("127.0.0.1:8080", "Connection refused");
    if let BlixardError::ConnectionError { address, details } = error {
        assert_eq!(address, "127.0.0.1:8080");
        assert_eq!(details, "Connection refused");
    }
}

#[test]
fn test_automatic_error_conversions() {
    // Test that standard error types automatically convert to BlixardError
    
    // UUID parsing error
    let uuid_error = Uuid::parse_str("invalid-uuid").unwrap_err();
    let blixard_error: BlixardError = uuid_error.into();
    if let BlixardError::InvalidInput { field, message } = blixard_error {
        assert_eq!(field, "uuid");
        assert!(message.contains("Invalid UUID format"));
    }

    // Address parsing error
    let addr_error = "invalid-address".parse::<SocketAddr>().unwrap_err();
    let blixard_error: BlixardError = addr_error.into();
    if let BlixardError::InvalidInput { field, message } = blixard_error {
        assert_eq!(field, "address");
        assert!(message.contains("Invalid network address"));
    }

    // Integer parsing error
    let int_error = "not-a-number".parse::<u64>().unwrap_err();
    let blixard_error: BlixardError = int_error.into();
    if let BlixardError::InvalidInput { field, message } = blixard_error {
        assert_eq!(field, "number");
        assert!(message.contains("Invalid number format"));
    }
}

#[test]
fn test_iroh_types_conversions() {
    use blixard_core::iroh_types::{NodeState as IrohNodeState, VmState as IrohVmState};
    use blixard_core::types::{NodeState as CoreNodeState, VmStatus as CoreVmStatus};

    // Test VmStatus -> VmState conversion
    let core_status = CoreVmStatus::Running;
    let iroh_state: IrohVmState = core_status.into();
    assert_eq!(iroh_state, IrohVmState::VmStateRunning);

    // Test VmState -> VmStatus conversion
    let iroh_state = IrohVmState::VmStateStopped;
    let core_status: CoreVmStatus = iroh_state.try_into().unwrap();
    assert_eq!(core_status, CoreVmStatus::Stopped);

    // Test unknown state conversion fails
    let unknown_state = IrohVmState::VmStateUnknown;
    let result = CoreVmStatus::try_from(unknown_state);
    assert!(result.is_err());

    // Test NodeState -> NodeState conversion
    let core_state = CoreNodeState::Active;
    let iroh_state: IrohNodeState = core_state.into();
    assert_eq!(iroh_state, IrohNodeState::NodeStateLeader);

    // Test Display implementations
    assert_eq!(IrohVmState::VmStateRunning.to_string(), "running");
    assert_eq!(IrohNodeState::NodeStateLeader.to_string(), "leader");
}

#[test]
fn test_comprehensive_roundtrip_conversions() {
    // Test that conversions are consistent in both directions where applicable
    
    // VmStatus roundtrip via string
    for status in [VmStatus::Creating, VmStatus::Running, VmStatus::Stopped, VmStatus::Failed] {
        let status_str = status.to_string();
        let parsed_status: VmStatus = status_str.parse().unwrap();
        assert_eq!(status, parsed_status);
    }

    // VmStatus roundtrip via i32
    for status in [VmStatus::Creating, VmStatus::Running, VmStatus::Stopped, VmStatus::Failed] {
        let status_code: i32 = status.into();
        let parsed_status: VmStatus = status_code.try_into().unwrap();
        assert_eq!(status, parsed_status);
    }

    // NodeState roundtrip via string
    for state in [NodeState::Initialized, NodeState::Active, NodeState::Error] {
        let state_str = state.to_string();
        let parsed_state: NodeState = state_str.parse().unwrap();
        assert_eq!(state, parsed_state);
    }

    // NodeId roundtrip
    for id_val in [1u64, 42u64, 999999u64] {
        let node_id = NodeId::from(id_val);
        let id_str = node_id.to_string();
        let parsed_id: NodeId = id_str.parse().unwrap();
        assert_eq!(node_id, parsed_id);
        assert_eq!(u64::from(parsed_id), id_val);
    }

    // VmId roundtrip
    let uuid = Uuid::new_v4();
    let vm_id = VmId::from(uuid);
    let id_str = vm_id.to_string();
    let parsed_id: VmId = id_str.parse().unwrap();
    assert_eq!(vm_id, parsed_id);
    assert_eq!(Uuid::from(parsed_id), uuid);
}