// Failpoint injection for testing
// This module provides failpoint utilities for deterministic testing

#[cfg(feature = "failpoints")]
pub use fail::{fail_point, FailScenario};

#[cfg(not(feature = "failpoints"))]
#[macro_export]
macro_rules! fail_point {
    ($name:expr) => {};
    ($name:expr, $closure:expr) => {};
}

#[cfg(not(feature = "failpoints"))]
pub struct FailScenario;

#[cfg(not(feature = "failpoints"))]
impl FailScenario {
    pub fn setup() -> Self {
        Self
    }

    pub fn teardown(self) {}
}
