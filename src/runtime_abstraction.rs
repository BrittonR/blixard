/// Runtime abstraction layer that uses the runtime context
/// This now properly switches between real and simulated runtime!
// Re-export from runtime_context
pub use crate::runtime_context::{RuntimeGuard, RuntimeHandle, SimulatedRuntimeHandle};

// Keep the simulation module for backward compatibility
#[cfg(feature = "simulation")]
pub mod simulated {
    use crate::runtime_context::set_global_runtime;
    use std::sync::Arc;

    pub fn init_simulation() {
        // Set up simulated runtime as global default
        let sim_handle = Arc::new(crate::runtime_context::SimulatedRuntimeHandle::new(42));
        set_global_runtime(sim_handle);
    }
}
