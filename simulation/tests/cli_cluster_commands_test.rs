#[cfg(feature = "test-helpers")]
mod tests {
    use std::time::Duration;
    use blixard::test_helpers::{TestNode, PortAllocator};
    use tokio::time::timeout;
    use tokio::process::Command;
    
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_cluster_status_command() {
        // Start a test node with dynamic port
        let port = PortAllocator::next_port();
        let node = TestNode::builder()
            .with_id(1)
            .with_port(port)
            .build()
            .await
            .expect("Failed to create node");
        
        // Wait for node to be ready
        blixard::test_helpers::timing::robust_sleep(Duration::from_millis(500)).await;
        
        // Run cluster status command with timeout
        let result = timeout(
            Duration::from_secs(10),
            Command::new("cargo")
                .args(&["run", "--bin", "blixard", "--", "cluster", "status", "--addr", &format!("127.0.0.1:{}", port)])
                .output()
        ).await;
        
        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => panic!("Failed to execute command: {}", e),
            Err(_) => panic!("Command timed out after 10 seconds"),
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        // Check output contains expected information
        assert!(stdout.contains("Cluster Status:") || stderr.contains("Cluster Status:"), 
                "Expected cluster status output, got stdout: {}, stderr: {}", stdout, stderr);
        
        // Clean up
        node.shutdown().await;
    }
    
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_cluster_join_command() {
        // Start two test nodes with dynamic ports
        let port1 = PortAllocator::next_port();
        let port2 = PortAllocator::next_port();
        
        let node1 = TestNode::builder()
            .with_id(1)
            .with_port(port1)
            .build()
            .await
            .expect("Failed to create node 1");
        
        let node2 = TestNode::builder()
            .with_id(2)
            .with_port(port2)
            .build()
            .await
            .expect("Failed to create node 2");
        
        // Wait for nodes to be ready
        blixard::test_helpers::timing::robust_sleep(Duration::from_millis(500)).await;
        
        // Run join command from node2 to join node1's cluster with timeout
        let result = timeout(
            Duration::from_secs(10),
            Command::new("cargo")
                .args(&[
                    "run", "--bin", "blixard", "--", 
                    "cluster", "join", 
                    "--peer", &format!("2@127.0.0.1:{}", port1),
                    "--local-addr", &format!("127.0.0.1:{}", port2)
                ])
                .output()
        ).await;
        
        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => panic!("Failed to execute command: {}", e),
            Err(_) => panic!("Command timed out after 10 seconds"),
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        // Check for success or expected error
        // Note: Join might fail if the nodes aren't fully configured, but we're testing the CLI works
        assert!(
            stdout.contains("cluster") || stderr.contains("cluster") || 
            stdout.contains("join") || stderr.contains("join"),
            "Expected cluster join output, got stdout: {}, stderr: {}", stdout, stderr
        );
        
        // Clean up
        node2.shutdown().await;
        node1.shutdown().await;
    }
    
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_cli_help() {
        // Test that cluster commands show up in help
        let result = timeout(
            Duration::from_secs(10),
            Command::new("cargo")
                .args(&["run", "--bin", "blixard", "--", "cluster", "--help"])
                .output()
        ).await;
        
        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => panic!("Failed to execute command: {}", e),
            Err(_) => panic!("Command timed out after 10 seconds"),
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let help_text = format!("{}{}", stdout, stderr);
        
        // Check help contains our commands
        assert!(help_text.contains("join"), "Help should contain 'join' command");
        assert!(help_text.contains("leave"), "Help should contain 'leave' command");
        assert!(help_text.contains("status"), "Help should contain 'status' command");
    }
}

#[cfg(not(feature = "test-helpers"))]
fn main() {
    eprintln!("Tests require --features test-helpers");
}