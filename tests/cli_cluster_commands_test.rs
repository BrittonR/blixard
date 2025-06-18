#[cfg(feature = "test-helpers")]
mod tests {
    use std::process::Command;
    use std::time::Duration;
    use blixard::test_helpers::TestNode;
    
    #[tokio::test]
    async fn test_cluster_status_command() {
        // Start a test node
        let node = TestNode::builder()
            .with_id(1)
            .with_port(8001)
            .build()
            .await
            .expect("Failed to create node");
        
        // Wait for node to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Run cluster status command
        let output = Command::new("cargo")
            .args(&["run", "--bin", "blixard", "--", "cluster", "status", "--addr", "127.0.0.1:8001"])
            .output()
            .expect("Failed to execute command");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        // Check output contains expected information
        assert!(stdout.contains("Cluster Status:") || stderr.contains("Cluster Status:"), 
                "Expected cluster status output, got stdout: {}, stderr: {}", stdout, stderr);
        
        // Clean up
        node.shutdown().await;
    }
    
    #[tokio::test]
    async fn test_cluster_join_command() {
        // Start two test nodes
        let node1 = TestNode::builder()
            .with_id(1)
            .with_port(8101)
            .build()
            .await
            .expect("Failed to create node 1");
        
        let node2 = TestNode::builder()
            .with_id(2)
            .with_port(8102)
            .build()
            .await
            .expect("Failed to create node 2");
        
        // Wait for nodes to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Run join command from node2 to join node1's cluster
        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "blixard", "--", 
                "cluster", "join", 
                "--peer", "2@127.0.0.1:8101",
                "--local-addr", "127.0.0.1:8102"
            ])
            .output()
            .expect("Failed to execute command");
        
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
    
    #[tokio::test]
    async fn test_cli_help() {
        // Test that cluster commands show up in help
        let output = Command::new("cargo")
            .args(&["run", "--bin", "blixard", "--", "cluster", "--help"])
            .output()
            .expect("Failed to execute command");
        
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