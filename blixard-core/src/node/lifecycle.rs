use std::time::Duration;
use tokio::sync::oneshot;

use crate::error::BlixardResult;

use super::core::Node;

impl Node {
    /// Start the node
    pub async fn start(&mut self) -> BlixardResult<()> {
        let node_id = self.shared.get_id();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shared.set_shutdown_tx(shutdown_tx);

        // The node doesn't need its own TCP listener since all communication
        // is handled via the gRPC server. We just need a task to manage the
        // node's lifecycle and respond to shutdown signals.
        let handle = tokio::spawn(async move {
            tracing::info!("Node {} started", node_id);

            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            tracing::info!("Node {} shutting down", node_id);

            Ok(())
        });

        self.handle = Some(handle);
        self.shared.set_running(true);

        // Start VM health monitor
        if let Some(ref mut monitor) = self.health_monitor {
            monitor.start();
            tracing::info!("VM health monitor started");
        }

        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(tx) = self.shared.take_shutdown_tx() {
            let _ = tx.send(());
        }

        // Stop main node handle
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(result) => result?,
                Err(_) => {} // Task was cancelled
            }
        }

        // Stop Raft handle
        if let Some(handle) = self.raft_handle.take() {
            handle.abort(); // Abort the Raft task
        }

        // Stop VM health monitor
        if let Some(ref mut monitor) = self.health_monitor {
            monitor.stop();
            tracing::info!("VM health monitor stopped");
        }

        // Shutdown Raft transport
        if let Some(transport) = self.raft_transport.take() {
            transport.shutdown().await;
            tracing::info!("Raft transport shut down");
        }

        self.shared.set_running(false);
        self.shared.set_initialized(false);

        // Shutdown all components to release database references
        if let Err(e) = self.shared.shutdown_components().await {
            tracing::warn!("Failed to shutdown components cleanly: {}", e);
        }

        // Add a small delay to ensure file locks are released
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(())
    }
}