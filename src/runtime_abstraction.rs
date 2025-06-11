/// Runtime abstraction layer that uses the runtime context
/// This now properly switches between real and simulated runtime!

// Re-export from runtime_context
pub use crate::runtime_context::{
    spawn,
    sleep,
    now,
    set_global_runtime,
    set_thread_runtime,
    clear_thread_runtime,
    with_simulated_runtime,
    RuntimeGuard,
    RuntimeHandle,
    SimulatedRuntimeHandle,
};

// Keep the simulation module for backward compatibility
#[cfg(feature = "simulation")]
pub mod simulated {
    use super::*;
    use std::sync::Arc;
    
    pub fn init_simulation() {
        // Set up simulated runtime as global default
        let sim_handle = Arc::new(crate::runtime_context::SimulatedRuntimeHandle::new(42));
        set_global_runtime(sim_handle);
    }
}