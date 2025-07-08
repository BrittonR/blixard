//! Tests for anti-affinity rule serialization

mod common;

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

    let vm_config = {
        let mut config = common::test_vm_config("test-vm");
        config.vcpus = 4;
        config.memory = 8192;
        config.ip_address = Some("10.0.0.1".to_string());
        config.anti_affinity = Some(anti_affinity);
        config
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
    let vm_config = {
        let mut config = common::test_vm_config("simple-vm");
        config.vcpus = 2;
        config.memory = 4096;
        config
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
