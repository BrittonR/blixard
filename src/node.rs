use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::Result;
use crate::types::NodeConfig;

/// A Blixard cluster node
pub struct Node {
    config: NodeConfig,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<Result<()>>>,
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            shutdown_tx: None,
            handle: None,
        }
    }

    /// Start the node
    pub async fn start(&mut self) -> Result<()> {
        let config = self.config.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            let listener = TcpListener::bind(&config.bind_addr).await?;
            tracing::info!("Node {} listening on {}", config.id, config.bind_addr);

            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                tracing::debug!("New connection from {}", addr);
                                // Handle connection
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::info!("Node {} shutting down", config.id);
                        break;
                    }
                }
            }

            Ok(())
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(result) => result,
                Err(_) => Ok(()), // Task was cancelled
            }
        } else {
            Ok(())
        }
    }

    /// Check if the node is running
    pub fn is_running(&self) -> bool {
        self.handle.as_ref().map_or(false, |h| !h.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_lifecycle() {
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/test".to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };

        let mut node = Node::new(config);
        
        // Start node
        node.start().await.unwrap();
        assert!(node.is_running());

        // Stop node
        node.stop().await.unwrap();
        assert!(!node.is_running());
    }
}