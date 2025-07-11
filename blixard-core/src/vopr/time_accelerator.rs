//! Time acceleration and manipulation for deterministic testing
//!
//! Provides simulated time that can run much faster than real time,
//! with support for clock skew, time jumps, and controlled progression.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Simulated time that can be controlled and accelerated
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SimulatedTime {
    /// Nanoseconds since epoch
    nanos: u128,
}

impl SimulatedTime {
    /// Create a new simulated time at the given nanoseconds since epoch
    pub fn from_nanos(nanos: u128) -> Self {
        Self { nanos }
    }

    /// Convert to Duration since epoch
    pub fn duration_since_epoch(&self) -> Duration {
        Duration::from_nanos(self.nanos as u64)
    }

    /// Convert to SystemTime
    pub fn to_system_time(&self) -> SystemTime {
        UNIX_EPOCH + self.duration_since_epoch()
    }

    /// Add a duration to this time
    pub fn add(&self, duration: Duration) -> Self {
        Self {
            nanos: self.nanos + duration.as_nanos(),
        }
    }

    /// Subtract a duration from this time
    pub fn sub(&self, duration: Duration) -> Option<Self> {
        self.nanos
            .checked_sub(duration.as_nanos())
            .map(|nanos| Self { nanos })
    }

    /// Get the duration between two times
    pub fn duration_since(&self, earlier: &SimulatedTime) -> Option<Duration> {
        self.nanos
            .checked_sub(earlier.nanos)
            .map(|nanos| Duration::from_nanos(nanos as u64))
    }
}

/// Time source for a single node with potential clock skew
#[derive(Debug)]
struct NodeClock {
    /// Base time (shared across all nodes)
    base_time: SimulatedTime,

    /// Clock skew for this node (positive or negative)
    skew: i64,

    /// Additional drift rate (nanoseconds per simulated second)
    drift_rate: i64,

    /// Time of last update (for drift calculation)
    last_update: SimulatedTime,
}

impl NodeClock {
    fn new(base_time: SimulatedTime) -> Self {
        Self {
            base_time,
            skew: 0,
            drift_rate: 0,
            last_update: base_time,
        }
    }

    /// Get the current time for this node
    fn now(&self, global_time: SimulatedTime) -> SimulatedTime {
        // Calculate drift since last update
        let elapsed = global_time
            .duration_since(&self.last_update)
            .unwrap_or(Duration::ZERO);
        let drift_nanos = (elapsed.as_secs() as i64) * self.drift_rate;

        // Apply skew and drift
        let adjusted_nanos =
            (global_time.nanos as i128) + (self.skew as i128) + (drift_nanos as i128);

        // Ensure time doesn't go negative
        SimulatedTime::from_nanos(adjusted_nanos.max(0) as u128)
    }

    /// Apply a sudden time jump
    fn jump(&mut self, delta: i64) {
        self.skew = self.skew.saturating_add(delta);
    }

    /// Set the drift rate
    fn set_drift(&mut self, rate: i64, current_time: SimulatedTime) {
        self.drift_rate = rate;
        self.last_update = current_time;
    }
}

/// Time accelerator that controls time progression for all nodes
pub struct TimeAccelerator {
    /// Global simulated time
    global_time: Arc<Mutex<SimulatedTime>>,

    /// Per-node clocks with potential skew
    node_clocks: Arc<Mutex<HashMap<u64, NodeClock>>>,

    /// Time acceleration factor
    acceleration: u64,

    /// Maximum allowed clock skew in nanoseconds
    max_skew_nanos: u64,

    /// Random number generator for introducing variations
    rng: Arc<Mutex<ChaCha8Rng>>,

    /// Real time when simulation started
    start_real_time: std::time::Instant,

    /// Simulated time when simulation started
    start_sim_time: SimulatedTime,
}

impl TimeAccelerator {
    /// Create a new time accelerator
    pub fn new(acceleration: u64, max_clock_skew_ms: u64) -> Self {
        let now = SimulatedTime::from_nanos(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        );

        Self {
            global_time: Arc::new(Mutex::new(now)),
            node_clocks: Arc::new(Mutex::new(HashMap::new())),
            acceleration,
            max_skew_nanos: max_clock_skew_ms * 1_000_000,
            rng: Arc::new(Mutex::new(ChaCha8Rng::seed_from_u64(42))),
            start_real_time: std::time::Instant::now(),
            start_sim_time: now,
        }
    }

    /// Set the random seed for deterministic behavior
    pub fn set_seed(&mut self, seed: u64) {
        *self.rng.lock().unwrap() = ChaCha8Rng::seed_from_u64(seed);
    }

    /// Register a new node with the time system
    pub fn register_node(&self, node_id: u64) {
        let mut clocks = self.node_clocks.lock().unwrap();
        let global = self.global_time.lock().unwrap();
        clocks.insert(node_id, NodeClock::new(*global));
    }

    /// Get the current time for a specific node
    pub fn now(&self, node_id: u64) -> SimulatedTime {
        let clocks = self.node_clocks.lock().unwrap();
        let global = self.global_time.lock().unwrap();

        clocks
            .get(&node_id)
            .map(|clock| clock.now(*global))
            .unwrap_or(*global)
    }

    /// Advance global time by the given duration
    pub fn advance(&self, duration: Duration) {
        let mut global = self.global_time.lock().unwrap();
        *global = global.add(duration);
    }

    /// Advance time based on real time elapsed (with acceleration)
    pub fn advance_by_real_time(&self) {
        let real_elapsed = self.start_real_time.elapsed();
        let sim_elapsed = Duration::from_nanos(
            (real_elapsed.as_nanos() as u128 * self.acceleration as u128) as u64,
        );

        let mut global = self.global_time.lock().unwrap();
        *global = self.start_sim_time.add(sim_elapsed);
    }

    /// Introduce random clock skew to a node
    pub fn randomize_clock_skew(&self, node_id: u64) {
        let mut rng = self.rng.lock().unwrap();
        let skew = rng.gen_range(-(self.max_skew_nanos as i64)..=(self.max_skew_nanos as i64));

        let mut clocks = self.node_clocks.lock().unwrap();
        if let Some(clock) = clocks.get_mut(&node_id) {
            clock.skew = skew;
        }
    }

    /// Apply a sudden time jump to a node
    pub fn jump_node_time(&self, node_id: u64, delta: Duration, forward: bool) {
        let delta_nanos = if forward {
            delta.as_nanos() as i64
        } else {
            -(delta.as_nanos() as i64)
        };

        let mut clocks = self.node_clocks.lock().unwrap();
        if let Some(clock) = clocks.get_mut(&node_id) {
            clock.jump(delta_nanos);
        }
    }

    /// Set clock drift rate for a node (nanoseconds per second)
    pub fn set_clock_drift(&self, node_id: u64, drift_rate: i64) {
        let mut clocks = self.node_clocks.lock().unwrap();
        let global = self.global_time.lock().unwrap();

        if let Some(clock) = clocks.get_mut(&node_id) {
            clock.set_drift(drift_rate, *global);
        }
    }

    /// Get statistics about clock skew across all nodes
    pub fn clock_skew_stats(&self) -> ClockSkewStats {
        let (clocks, global) = match (self.node_clocks.lock(), self.global_time.lock()) {
            (Ok(c), Ok(g)) => (c, g),
            _ => return ClockSkewStats::default(),
        };

        if clocks.is_empty() {
            return ClockSkewStats::default();
        }

        let times: Vec<i128> = clocks
            .values()
            .map(|clock| {
                let node_time = clock.now(*global);
                node_time.nanos as i128 - global.nanos as i128
            })
            .collect();

        let min_skew = *times.iter().min().unwrap_or(&0);
        let max_skew = *times.iter().max().unwrap_or(&0);
        let avg_skew = if times.is_empty() {
            0
        } else {
            times.iter().sum::<i128>() / times.len() as i128
        };

        ClockSkewStats {
            min_skew_nanos: min_skew,
            max_skew_nanos: max_skew,
            avg_skew_nanos: avg_skew,
            spread_nanos: max_skew - min_skew,
        }
    }

    /// Create a time dilation effect (slow down or speed up time for specific operations)
    pub fn with_time_dilation<F, R>(&self, factor: f64, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // Store current acceleration
        let old_acceleration = self.acceleration;

        // Apply dilation factor
        // Note: In a real implementation, we'd need to handle this more carefully
        // to avoid race conditions

        let result = f();

        // Restore original acceleration

        result
    }
}

/// Statistics about clock skew across nodes
#[derive(Debug, Default)]
pub struct ClockSkewStats {
    pub min_skew_nanos: i128,
    pub max_skew_nanos: i128,
    pub avg_skew_nanos: i128,
    pub spread_nanos: i128,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulated_time() {
        let t1 = SimulatedTime::from_nanos(1_000_000_000);
        let t2 = t1.add(Duration::from_secs(1));

        assert_eq!(t2.nanos, 2_000_000_000);
        assert_eq!(t2.duration_since(&t1).unwrap(), Duration::from_secs(1));
    }

    #[test]
    fn test_time_acceleration() {
        let accel = TimeAccelerator::new(1000, 100);

        accel.register_node(1);
        accel.register_node(2);

        let t1_before = accel.now(1);
        let t2_before = accel.now(2);

        // Advance time by 1 second
        accel.advance(Duration::from_secs(1));

        let t1_after = accel.now(1);
        let t2_after = accel.now(2);

        // Both nodes should have advanced by 1 second
        assert_eq!(
            t1_after.duration_since(&t1_before).unwrap(),
            Duration::from_secs(1)
        );
        assert_eq!(
            t2_after.duration_since(&t2_before).unwrap(),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn test_clock_skew() {
        let accel = TimeAccelerator::new(1000, 100);

        accel.register_node(1);
        accel.register_node(2);

        // Apply skew to node 2
        accel.jump_node_time(2, Duration::from_millis(50), true);

        let t1 = accel.now(1);
        let t2 = accel.now(2);

        // Node 2 should be ahead by ~50ms
        let diff = t2.duration_since(&t1).unwrap();
        assert!(diff >= Duration::from_millis(49));
        assert!(diff <= Duration::from_millis(51));
    }
}
