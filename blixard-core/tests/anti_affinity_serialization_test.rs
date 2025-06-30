//! Tests for anti-affinity rule serialization

use blixard_core::{
    anti_affinity::{AntiAffinityRule, AntiAffinityRules, AntiAffinityType},
    types::VmConfig,
};

#[test]
fn test_anti_affinity_rule_serialization() {
    let rule = AntiAffinityRule::hard("web-app")
        .with_max_per_node(2)
        .with_scope("rack");
    
    let json = serde_json::to_string_pretty(&rule).unwrap();
    println!("Serialized rule:\n{}", json);
    
    let deserialized: AntiAffinityRule = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.group_key, "web-app");
    assert_eq!(deserialized.max_per_node, 2);
    assert_eq!(deserialized.scope.as_deref(), Some("rack"));
    assert!(matches!(deserialized.rule_type, AntiAffinityType::Hard));
}

#[test]
fn test_vm_config_with_anti_affinity_serialization() {
    let anti_affinity = AntiAffinityRules::new()
        .add_rule(AntiAffinityRule::hard("frontend"))
        .add_rule(AntiAffinityRule::soft("cache", 0.5));
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/etc/vm.nix".to_string(),
        vcpus: 4,
        memory: 8192,
        tenant_id: "test-tenant".to_string(),
        ip_address: Some("10.0.0.1".to_string()),
        metadata: None,
        anti_affinity: Some(anti_affinity),
    };
    
    let json = serde_json::to_string_pretty(&vm_config).unwrap();
    println!("Serialized VM config:\n{}", json);
    
    let deserialized: VmConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "test-vm");
    assert!(deserialized.anti_affinity.is_some());
    
    let rules = deserialized.anti_affinity.unwrap();
    assert_eq!(rules.rules.len(), 2);
    assert_eq!(rules.rules[0].group_key, "frontend");
    assert_eq!(rules.rules[1].group_key, "cache");
}

#[test]
fn test_vm_config_without_anti_affinity() {
    let vm_config = VmConfig {
        name: "simple-vm".to_string(),
        config_path: "/etc/vm.nix".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
    };
    
    let json = serde_json::to_string(&vm_config).unwrap();
    // anti_affinity field should be omitted when None
    assert!(!json.contains("anti_affinity"));
    
    let deserialized: VmConfig = serde_json::from_str(&json).unwrap();
    assert!(deserialized.anti_affinity.is_none());
}

#[test]
fn test_anti_affinity_default_values() {
    // Test that default values work correctly
    let json = r#"{
        "rule_type": "Hard",
        "group_key": "database"
    }"#;
    
    let rule: AntiAffinityRule = serde_json::from_str(json).unwrap();
    assert_eq!(rule.max_per_node, 1); // Should use default
    assert!(rule.scope.is_none()); // Should be None
}

#[test]
fn test_soft_anti_affinity_weight() {
    let rule = AntiAffinityRule::soft("test", 2.5);
    let json = serde_json::to_string(&rule).unwrap();
    
    let deserialized: AntiAffinityRule = serde_json::from_str(&json).unwrap();
    if let AntiAffinityType::Soft { weight } = deserialized.rule_type {
        assert_eq!(weight, 2.5);
    } else {
        panic!("Expected soft rule type");
    }
}