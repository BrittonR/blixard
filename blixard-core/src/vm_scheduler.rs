/// VM Scheduler module providing intelligent VM placement across cluster nodes
/// 
/// This module has been split into focused submodules:
/// - `placement_strategies`: All placement algorithms and decision logic
/// - `resource_analysis`: Resource usage analysis and node evaluation
/// - `mod`: Main coordinator and VmScheduler struct
// Re-export everything from the modular implementation  
pub use crate::vm_scheduler_modules::*;