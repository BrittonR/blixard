#![cfg(feature = "failpoints")]

use blixard::microvm::MicroVm;
use blixard::types::*;
use fail::{fail_point, FailScenario};

// This test doesn't need simulation - it's testing real failpoint injection
#[tokio::test]
async fn test_vm_start_with_failures() {
    let _scenario = FailScenario::setup();

    // Configure 50% failure rate for VM starts
    fail::cfg("microvm_start", "50%return").unwrap();

    let mut successes = 0;
    let mut failures = 0;

    // Try to start VMs 100 times
    for i in 0..100 {
        let result = start_vm_with_fault_injection(&format!("test-vm-{}", i)).await;
        match result {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }

    // Should be roughly 50/50
    assert!(successes > 40 && successes < 60);
    assert!(failures > 40 && failures < 60);
}

async fn start_vm_with_fault_injection(name: &str) -> Result<(), String> {
    fail_point!("microvm_start", |_| {
        Err("Simulated VM start failure".to_string())
    });

    // Normal VM start logic would go here
    Ok(())
}

// This test doesn't need simulation either - it's testing storage corruption handling
#[tokio::test]
async fn test_storage_corruption_handling() {
    let _scenario = FailScenario::setup();

    // Inject storage corruption
    fail::cfg("storage_read", "1*return(corrupt)").unwrap();

    // Test should handle corruption gracefully
    // TODO: Implement actual storage corruption test
    assert!(true);
}
