pub mod orchestrator;
pub mod tui;
pub mod node_discovery;
pub mod discovery_manager;

pub use orchestrator::BlixardOrchestrator;

// Re-export commonly used types
pub use blixard_core::{
    error::{BlixardError, BlixardResult},
    types::{NodeConfig, VmConfig, VmStatus},
    node::Node,
};

pub use blixard_vm::{VmBackend, MicrovmBackend};