//! P2P utilities for bandwidth tracking and document management
//!
//! This module provides shared utilities used across P2P transport implementations
//! to avoid code duplication and maintain consistency.

use crate::p2p_monitor::Direction;
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime};

/// Document entry for P2P storage
/// 
/// Represents a key-value document with metadata for P2P sharing
#[derive(Debug, Clone)]
pub struct DocumentEntry {
    /// Document key for lookup
    pub key: String,
    /// Document value/content
    pub value: Vec<u8>,
    /// Author identifier (e.g., node ID)
    pub author: String,
    /// Creation/modification timestamp
    pub timestamp: SystemTime,
}

/// Bandwidth tracking entry
#[derive(Debug, Clone)]
struct BandwidthEntry {
    timestamp: Instant,
    bytes: u64,
    direction: Direction,
}

/// Time-windowed bandwidth tracker for monitoring P2P data transfer rates
///
/// This tracker maintains a rolling window of bandwidth measurements,
/// automatically cleaning up old entries and providing current bandwidth
/// calculations for both inbound and outbound traffic.
#[derive(Debug)]
pub struct BandwidthTracker {
    /// Rolling window of bandwidth entries
    entries: VecDeque<BandwidthEntry>,
    /// Window duration for measurements
    window: Duration,
    /// Total bytes in current window (inbound)
    total_bytes_in: u64,
    /// Total bytes in current window (outbound)
    total_bytes_out: u64,
}

impl BandwidthTracker {
    /// Create a new bandwidth tracker with the specified window duration
    pub fn new(window: Duration) -> Self {
        Self {
            entries: VecDeque::new(),
            window,
            total_bytes_in: 0,
            total_bytes_out: 0,
        }
    }

    /// Add a new bandwidth entry
    pub fn add_entry(&mut self, bytes: u64, direction: Direction) {
        let now = Instant::now();
        self.cleanup_old_entries(now);

        self.entries.push_back(BandwidthEntry {
            timestamp: now,
            bytes,
            direction,
        });

        match direction {
            Direction::Inbound => self.total_bytes_in += bytes,
            Direction::Outbound => self.total_bytes_out += bytes,
        }
    }

    /// Remove entries older than the window duration
    fn cleanup_old_entries(&mut self, now: Instant) {
        while let Some(front) = self.entries.front() {
            if now.duration_since(front.timestamp) > self.window {
                // Safe to pop since we just checked front() exists
                if let Some(entry) = self.entries.pop_front() {
                    match entry.direction {
                        Direction::Inbound => self.total_bytes_in -= entry.bytes,
                        Direction::Outbound => self.total_bytes_out -= entry.bytes,
                    }
                }
            } else {
                break;
            }
        }
    }

    /// Get current bandwidth in bytes per second (inbound, outbound)
    pub fn get_bandwidth(&mut self) -> (f64, f64) {
        let now = Instant::now();
        self.cleanup_old_entries(now);

        let window_secs = self.window.as_secs_f64();
        let bytes_per_sec_in = self.total_bytes_in as f64 / window_secs;
        let bytes_per_sec_out = self.total_bytes_out as f64 / window_secs;

        (bytes_per_sec_in, bytes_per_sec_out)
    }

    /// Get total bytes transferred in the current window
    pub fn get_total_bytes(&self) -> (u64, u64) {
        (self.total_bytes_in, self.total_bytes_out)
    }

    /// Reset all measurements
    pub fn reset(&mut self) {
        self.entries.clear();
        self.total_bytes_in = 0;
        self.total_bytes_out = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_bandwidth_tracker() {
        let mut tracker = BandwidthTracker::new(Duration::from_secs(1));
        
        // Add some entries
        tracker.add_entry(1000, Direction::Inbound);
        tracker.add_entry(2000, Direction::Outbound);
        
        // Check totals
        let (total_in, total_out) = tracker.get_total_bytes();
        assert_eq!(total_in, 1000);
        assert_eq!(total_out, 2000);
        
        // Check bandwidth (should be bytes/sec over 1 second window)
        let (bw_in, bw_out) = tracker.get_bandwidth();
        assert_eq!(bw_in, 1000.0);
        assert_eq!(bw_out, 2000.0);
    }

    #[test]
    fn test_bandwidth_window_cleanup() {
        let mut tracker = BandwidthTracker::new(Duration::from_millis(100));
        
        // Add entry
        tracker.add_entry(1000, Direction::Inbound);
        
        // Wait for window to expire
        thread::sleep(Duration::from_millis(150));
        
        // Add new entry (should trigger cleanup)
        tracker.add_entry(500, Direction::Inbound);
        
        // Only new entry should be counted
        let (total_in, _) = tracker.get_total_bytes();
        assert_eq!(total_in, 500);
    }

    #[test]
    fn test_document_entry() {
        let entry = DocumentEntry {
            key: "test-key".to_string(),
            value: b"test-value".to_vec(),
            author: "node-1".to_string(),
            timestamp: SystemTime::now(),
        };
        
        assert_eq!(entry.key, "test-key");
        assert_eq!(entry.value, b"test-value");
        assert_eq!(entry.author, "node-1");
    }
}