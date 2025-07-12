//! Message filtering for network partition simulation in tests
//!
//! This module provides a way to simulate network partitions by
//! intercepting and filtering messages between nodes.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A filter that determines whether a message should be delivered
pub trait MessageFilter: Send + Sync {
    /// Returns true if the message should be delivered, false to drop it
    fn should_deliver(&self, from: u64, to: u64) -> bool;
}

/// A simple partition filter that blocks messages between partition groups
#[derive(Clone)]
pub struct PartitionFilter {
    partitions: Arc<RwLock<Vec<HashSet<u64>>>>,
}

impl PartitionFilter {
    pub fn new() -> Self {
        Self {
            partitions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create a partition where nodes in different groups cannot communicate
    pub async fn set_partitions(&self, groups: Vec<Vec<u64>>) {
        let mut partitions = self.partitions.write().await;
        partitions.clear();
        for group in groups {
            partitions.push(group.into_iter().collect());
        }
    }

    /// Remove all partitions, allowing full communication
    pub async fn heal_partitions(&self) {
        let mut partitions = self.partitions.write().await;
        partitions.clear();
    }

    /// Check if two nodes are in the same partition
    async fn in_same_partition(&self, from: u64, to: u64) -> bool {
        let partitions = self.partitions.read().await;

        // If no partitions defined, all nodes can communicate
        if partitions.is_empty() {
            return true;
        }

        // Find which partition each node is in
        for partition in partitions.iter() {
            if partition.contains(&from) && partition.contains(&to) {
                return true;
            }
        }

        // Nodes in different partitions cannot communicate
        false
    }
}

impl MessageFilter for PartitionFilter {
    fn should_deliver(&self, from: u64, to: u64) -> bool {
        // Use block_on since trait doesn't support async
        // In production, we'd design this differently
        futures::executor::block_on(self.in_same_partition(from, to))
    }
}

/// A filter that randomly drops messages to simulate unreliable network
pub struct RandomDropFilter {
    drop_rate: f64,
}

impl RandomDropFilter {
    pub fn new(drop_rate: f64) -> Self {
        assert!(drop_rate >= 0.0 && drop_rate <= 1.0);
        Self { drop_rate }
    }
}

impl MessageFilter for RandomDropFilter {
    fn should_deliver(&self, _from: u64, _to: u64) -> bool {
        rand::random::<f64>() > self.drop_rate
    }
}

/// A filter that adds latency to messages (would need async support)
pub struct LatencyFilter {
    /// Latency to add in milliseconds
    /// Currently unused pending async message filtering implementation
    #[allow(dead_code)]
    latency_ms: u64,
}

impl LatencyFilter {
    pub fn new(latency_ms: u64) -> Self {
        Self { latency_ms }
    }
}

impl MessageFilter for LatencyFilter {
    fn should_deliver(&self, _from: u64, _to: u64) -> bool {
        // For now, just return true
        // Real implementation would delay the message
        true
    }
}

/// Composite filter that combines multiple filters
pub struct CompositeFilter {
    filters: Vec<Box<dyn MessageFilter>>,
}

impl CompositeFilter {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn add_filter(&mut self, filter: Box<dyn MessageFilter>) {
        self.filters.push(filter);
    }
}

impl MessageFilter for CompositeFilter {
    fn should_deliver(&self, from: u64, to: u64) -> bool {
        // All filters must allow the message
        self.filters.iter().all(|f| f.should_deliver(from, to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_partition_filter() {
        let filter = PartitionFilter::new();

        // No partitions - all nodes can communicate
        assert!(filter.should_deliver(1, 2));
        assert!(filter.should_deliver(2, 3));

        // Create partitions: [1,2] and [3,4]
        filter.set_partitions(vec![vec![1, 2], vec![3, 4]]).await;

        // Same partition - can communicate
        assert!(filter.should_deliver(1, 2));
        assert!(filter.should_deliver(2, 1));
        assert!(filter.should_deliver(3, 4));
        assert!(filter.should_deliver(4, 3));

        // Different partitions - cannot communicate
        assert!(!filter.should_deliver(1, 3));
        assert!(!filter.should_deliver(2, 4));
        assert!(!filter.should_deliver(3, 1));
        assert!(!filter.should_deliver(4, 2));

        // Heal partitions
        filter.heal_partitions().await;

        // All can communicate again
        assert!(filter.should_deliver(1, 3));
        assert!(filter.should_deliver(2, 4));
    }

    #[test]
    fn test_random_drop_filter() {
        // 100% drop rate
        let filter = RandomDropFilter::new(1.0);
        assert!(!filter.should_deliver(1, 2));

        // 0% drop rate
        let filter = RandomDropFilter::new(0.0);
        assert!(filter.should_deliver(1, 2));

        // 50% drop rate - should see both outcomes over many attempts
        let filter = RandomDropFilter::new(0.5);
        let mut delivered = 0;
        let attempts = 1000;

        for _ in 0..attempts {
            if filter.should_deliver(1, 2) {
                delivered += 1;
            }
        }

        // Should be roughly 50%
        assert!(delivered > 400 && delivered < 600);
    }
}
