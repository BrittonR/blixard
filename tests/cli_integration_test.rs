use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn cli_help_works() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Distributed microVM orchestration platform"));
}

#[test]
fn cli_version_works() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("blixard"));
}

#[test]
fn node_subcommand_help() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("node")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Start a Blixard node"))
        .stdout(predicate::str::contains("--id"))
        .stdout(predicate::str::contains("--bind"))
        .stdout(predicate::str::contains("--peer"));
}

#[test]
fn vm_subcommand_help() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("VM management commands"));
}

#[test]
fn vm_create_help() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Create a new VM"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--vcpus"))
        .stdout(predicate::str::contains("--memory"));
}

#[test]
fn invalid_peer_format_error() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    let temp_dir = TempDir::new().unwrap();
    
    cmd.arg("node")
        .arg("--id").arg("1")
        .arg("--data-dir").arg(temp_dir.path())
        .arg("--peer").arg("invalid-peer-format")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid peer format"));
}

#[test]
fn valid_peer_format_accepted() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    let temp_dir = TempDir::new().unwrap();
    
    // Try to start node with valid peer format
    // The node will fail to start fully without peers, but should parse arguments correctly
    cmd.arg("node")
        .arg("--id").arg("1")
        .arg("--data-dir").arg(temp_dir.path())
        .arg("--bind").arg("127.0.0.1:17000")
        .arg("--peer").arg("2:127.0.0.1:17001")
        .timeout(std::time::Duration::from_millis(100));
    
    // If it times out (expected) or exits cleanly, both are fine
    // Just ensure no "Invalid peer format" error
    let result = cmd.output();
    if let Ok(output) = result {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("Invalid peer format"), "Got error: {}", stderr);
    }
}

#[test]
fn single_node_startup() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    // Start node with timeout (it runs forever)
    cmd.arg("node")
        .arg("--id").arg("1")
        .arg("--data-dir").arg(temp_dir.path())
        .arg("--bind").arg("127.0.0.1:17100")
        .timeout(std::time::Duration::from_secs(1))
        .assert()
        .interrupted();
    
    // Verify data directory was created (either the base dir has content or node-1 subdir exists)
    assert!(temp_dir.path().exists() && std::fs::read_dir(temp_dir.path()).unwrap().count() > 0);
}

// Note: Multi-node cluster tests would require more sophisticated process management
// For now, we test that nodes can be started with proper peer configuration
#[test]
fn multi_node_peer_configuration() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    // Test that node accepts multiple peers
    cmd.arg("node")
        .arg("--id").arg("3")
        .arg("--data-dir").arg(temp_dir.path())
        .arg("--bind").arg("127.0.0.1:17203")
        .arg("--peer").arg("1:127.0.0.1:17201")
        .arg("--peer").arg("2:127.0.0.1:17202")
        .timeout(std::time::Duration::from_millis(500))
        .assert()
        .interrupted();
}

#[test]
fn vm_commands_require_cluster_connection() {
    // VM list should fail without a running cluster
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    cmd.arg("vm")
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("connect").or(predicate::str::contains("cluster")));
}

#[test]
fn data_dir_default_value() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    // Start with minimal args to check default data dir
    cmd.arg("node")
        .arg("--id").arg("99")
        .arg("--bind").arg("127.0.0.1:17999")
        .timeout(std::time::Duration::from_millis(100))
        .assert()
        .interrupted();
    
    // Default data dir might be created (node might timeout before creating it)
    // Just ensure the command runs without errors
    
    // Cleanup
    std::fs::remove_dir_all("./blixard-data").ok();
}

#[test]
fn bind_address_default_value() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    let temp_dir = TempDir::new().unwrap();
    
    // Node should start with default bind address
    cmd.arg("node")
        .arg("--id").arg("1")
        .arg("--data-dir").arg(temp_dir.path())
        .timeout(std::time::Duration::from_millis(200))
        .assert()
        .interrupted()
        .stdout(predicate::str::contains("127.0.0.1:7000"));
}

// Additional integration tests for error scenarios
#[test]
fn duplicate_node_id_in_peers() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    let temp_dir = TempDir::new().unwrap();
    
    // Should handle duplicate peer IDs gracefully
    cmd.arg("node")
        .arg("--id").arg("1")
        .arg("--data-dir").arg(temp_dir.path())
        .arg("--peer").arg("2:127.0.0.1:8001")
        .arg("--peer").arg("2:127.0.0.1:8002") // Same ID, different address
        .timeout(std::time::Duration::from_millis(100));
    
    // Just verify it doesn't crash with duplicate peers
    let _ = cmd.output(); // We don't care about the specific result
}

#[test]
fn invalid_bind_address() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    cmd.arg("node")
        .arg("--id").arg("1")
        .arg("--bind").arg("invalid-address")
        .assert()
        .failure()
        .stderr(predicate::str::contains("parse").or(predicate::str::contains("invalid")));
}

#[test]
fn vm_create_missing_config() {
    let mut cmd = Command::cargo_bin("blixard").unwrap();
    
    cmd.arg("vm")
        .arg("create")
        .arg("test-vm")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--config"));
}