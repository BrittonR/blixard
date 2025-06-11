// CLI command parsing and validation tests

use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn test_cli_help_displays() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Distributed microVM orchestration platform"));
}

#[test]
fn test_node_command_requires_id() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("node")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_node_command_with_valid_id() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["node", "--id", "1"])
        .assert()
        .failure() // Should fail because functionality not implemented
        .stderr(predicate::str::contains("Node functionality not yet implemented"));
}

#[test]
fn test_vm_create_requires_name() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["vm", "create"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_vm_create_with_valid_name() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["vm", "create", "--name", "test-vm"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("VM creation not yet implemented"));
}

#[test]
fn test_vm_list_command() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["vm", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("VM listing not yet implemented"));
}

#[test]
fn test_invalid_subcommand() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("invalid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn test_node_id_edge_cases() {
    // Test with zero ID
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["node", "--id", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Node functionality not yet implemented"));

    // Test with very large ID
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["node", "--id", "18446744073709551615"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Node functionality not yet implemented"));
}

#[test]
fn test_vm_name_edge_cases() {
    // Test with empty name
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["vm", "create", "--name", ""])
        .assert()
        .failure();

    // Test with special characters
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.args(&["vm", "create", "--name", "test@vm"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("VM creation not yet implemented"));
}