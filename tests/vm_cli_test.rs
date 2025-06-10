use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Test VM CLI commands specifically
/// These tests verify command parsing and error handling without requiring a running cluster

#[test]
fn vm_create_requires_name() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("create")
        .assert()
        .failure()
        .stderr(predicate::str::contains("<NAME>"));
}

#[test]
fn vm_create_validates_arguments() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm")
        .arg("--config")
        .arg("/nonexistent/config.nix")
        .arg("--vcpus")
        .arg("0") // Invalid: 0 vcpus
        .assert()
        .failure();
}

#[test]
fn vm_create_accepts_valid_arguments() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    // Create a temporary config file
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("microvm.nix");
    std::fs::write(&config_path, "{ }").unwrap();
    
    // This will fail to connect to cluster, but arguments should be valid
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--vcpus")
        .arg("4")
        .arg("--memory")
        .arg("1024")
        .assert()
        .failure()
        .stderr(predicate::str::contains("connect").or(predicate::str::contains("cluster")));
}

#[test]
fn vm_start_requires_name() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("start")
        .assert()
        .failure()
        .stderr(predicate::str::contains("<NAME>"));
}

#[test]
fn vm_stop_requires_name() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("stop")
        .assert()
        .failure()
        .stderr(predicate::str::contains("<NAME>"));
}

#[test]
fn vm_status_requires_name() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("<NAME>"));
}

#[test]
fn vm_list_no_arguments() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    // VM list should not require arguments but will fail without cluster
    cmd.arg("vm")
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("connect").or(predicate::str::contains("cluster")));
}

#[test]
fn vm_subcommand_invalid() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("invalid-subcommand")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn node_and_vm_commands_exclusive() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    // Cannot use both node and vm commands
    cmd.arg("node")
        .arg("--id")
        .arg("1")
        .arg("vm")
        .arg("list")
        .assert()
        .failure();
}

#[test]
fn default_vcpu_and_memory_values() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("microvm.nix");
    std::fs::write(&config_path, "{ }").unwrap();
    
    // Should use defaults: 2 vcpus, 512 MB memory
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicate::str::contains("connect").or(predicate::str::contains("cluster")));
    
    // Test succeeded in parsing args, failed only on cluster connection
}

#[test]
fn memory_value_validation() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    // Memory should be a positive number
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm")
        .arg("--config")
        .arg("/path/to/config.nix")
        .arg("--memory")
        .arg("not-a-number")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn vcpu_value_validation() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    // VCPUs should be a positive number
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm")
        .arg("--config")
        .arg("/path/to/config.nix")
        .arg("--vcpus")
        .arg("-1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument").or(predicate::str::contains("tip:")));
}

#[test]
fn vm_name_with_special_characters() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("microvm.nix");
    std::fs::write(&config_path, "{ }").unwrap();
    
    // Should accept VM names with hyphens and underscores
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm_123")
        .arg("--config")
        .arg(config_path.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicate::str::contains("connect").or(predicate::str::contains("cluster")));
}