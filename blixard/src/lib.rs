pub mod client;
pub mod discovery_manager;
pub mod node_discovery;
pub mod orchestrator;
pub mod tui;

pub use orchestrator::BlixardOrchestrator;

// Re-export commonly used types
pub use blixard_core::{
    error::{BlixardError, BlixardResult},
    node::Node,
    types::{NodeConfig, VmConfig, VmStatus},
};

pub use blixard_vm::{MicrovmBackend, VmBackend};
