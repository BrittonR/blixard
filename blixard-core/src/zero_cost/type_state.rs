//! Type-state pattern for compile-time state machine validation
//!
//! This module provides zero-cost state machines where invalid state
//! transitions are caught at compile time.

use std::marker::PhantomData;

/// Trait for type-state pattern
pub trait TypeState: private::Sealed {
    /// A unique identifier for this state
    const STATE_NAME: &'static str;
}

/// Trait for valid state transitions
pub trait StateTransition<From: TypeState, To: TypeState>: private::Sealed {
    /// Perform the state transition
    fn transition(from: From) -> To;
}

mod private {
    pub trait Sealed {}
}

/// Example: VM lifecycle state machine with compile-time guarantees
pub mod vm_lifecycle {
    use super::*;
    use std::sync::Arc;
    use crate::types::VmConfig;

    /// Shared VM data across all states
    #[derive(Debug, Clone)]
    pub struct VmData {
        pub id: String,
        pub config: Arc<VmConfig>,
    }

    /// VM state machine
    pub struct Vm<S: TypeState> {
        data: Arc<VmData>,
        _state: PhantomData<S>,
    }

    // Define states
    pub struct Created;
    pub struct Configured;
    pub struct Starting;
    pub struct Running;
    pub struct Stopping;
    pub struct Stopped;
    pub struct Failed { error: String }

    // Implement TypeState for each state
    impl private::Sealed for Created {}
    impl TypeState for Created {
        const STATE_NAME: &'static str = "Created";
    }

    impl private::Sealed for Configured {}
    impl TypeState for Configured {
        const STATE_NAME: &'static str = "Configured";
    }

    impl private::Sealed for Starting {}
    impl TypeState for Starting {
        const STATE_NAME: &'static str = "Starting";
    }

    impl private::Sealed for Running {}
    impl TypeState for Running {
        const STATE_NAME: &'static str = "Running";
    }

    impl private::Sealed for Stopping {}
    impl TypeState for Stopping {
        const STATE_NAME: &'static str = "Stopping";
    }

    impl private::Sealed for Stopped {}
    impl TypeState for Stopped {
        const STATE_NAME: &'static str = "Stopped";
    }

    impl private::Sealed for Failed {}
    impl TypeState for Failed {
        const STATE_NAME: &'static str = "Failed";
    }

    // Define valid transitions
    impl Vm<Created> {
        /// Create a new VM in Created state
        pub fn new(id: String, config: VmConfig) -> Self {
            Self {
                data: Arc::new(VmData {
                    id,
                    config: Arc::new(config),
                }),
                _state: PhantomData,
            }
        }

        /// Configure the VM (Created -> Configured)
        pub fn configure(self) -> Vm<Configured> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }
    }

    impl Vm<Configured> {
        /// Start the VM (Configured -> Starting)
        pub fn start(self) -> Vm<Starting> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }
    }

    impl Vm<Starting> {
        /// VM successfully started (Starting -> Running)
        pub fn started(self) -> Vm<Running> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }

        /// VM failed to start (Starting -> Failed)
        pub fn failed(self, _error: String) -> Vm<Failed> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }
    }

    impl Vm<Running> {
        /// Stop the VM (Running -> Stopping)
        pub fn stop(self) -> Vm<Stopping> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }

        /// VM crashed (Running -> Failed)
        pub fn crashed(self, _error: String) -> Vm<Failed> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }
    }

    impl Vm<Stopping> {
        /// VM successfully stopped (Stopping -> Stopped)
        pub fn stopped(self) -> Vm<Stopped> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }

        /// Stop failed (Stopping -> Failed)
        pub fn stop_failed(self, _error: String) -> Vm<Failed> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }
    }

    impl Vm<Stopped> {
        /// Restart the VM (Stopped -> Starting)
        pub fn restart(self) -> Vm<Starting> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }

        /// Reconfigure the VM (Stopped -> Configured)
        pub fn reconfigure(self) -> Vm<Configured> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }
    }

    impl Vm<Failed> {
        /// Attempt recovery (Failed -> Configured)
        pub fn recover(self) -> Vm<Configured> {
            Vm {
                data: self.data,
                _state: PhantomData,
            }
        }

        /// Get the error message
        pub fn error(&self) -> &str {
            // In a real implementation, we'd store this in the state
            "Failed state"
        }
    }

    // Methods available in all states
    impl<S: TypeState> Vm<S> {
        /// Get the VM ID
        pub fn id(&self) -> &str {
            &self.data.id
        }

        /// Get the current state name
        pub fn state_name(&self) -> &'static str {
            S::STATE_NAME
        }

        /// Get VM configuration
        pub fn config(&self) -> &VmConfig {
            &self.data.config
        }
    }
}

/// Example: Node state machine
pub mod node_lifecycle {
    use super::*;

    pub struct Uninitialized;
    pub struct Initializing;
    pub struct Ready;
    pub struct JoiningCluster;
    pub struct InCluster;
    pub struct LeavingCluster;
    pub struct Shutdown;

    // Node state machine
    pub struct Node<S: TypeState> {
        id: u64,
        _state: PhantomData<S>,
    }

    impl private::Sealed for Uninitialized {}
    impl TypeState for Uninitialized {
        const STATE_NAME: &'static str = "Uninitialized";
    }

    impl private::Sealed for Initializing {}
    impl TypeState for Initializing {
        const STATE_NAME: &'static str = "Initializing";
    }

    impl private::Sealed for Ready {}
    impl TypeState for Ready {
        const STATE_NAME: &'static str = "Ready";
    }

    impl private::Sealed for JoiningCluster {}
    impl TypeState for JoiningCluster {
        const STATE_NAME: &'static str = "JoiningCluster";
    }

    impl private::Sealed for InCluster {}
    impl TypeState for InCluster {
        const STATE_NAME: &'static str = "InCluster";
    }

    impl private::Sealed for LeavingCluster {}
    impl TypeState for LeavingCluster {
        const STATE_NAME: &'static str = "LeavingCluster";
    }

    impl private::Sealed for Shutdown {}
    impl TypeState for Shutdown {
        const STATE_NAME: &'static str = "Shutdown";
    }

    impl Node<Uninitialized> {
        pub fn new(id: u64) -> Self {
            Self {
                id,
                _state: PhantomData,
            }
        }

        pub fn initialize(self) -> Node<Initializing> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }
    }

    impl Node<Initializing> {
        pub fn initialized(self) -> Node<Ready> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }
    }

    impl Node<Ready> {
        pub fn join_cluster(self) -> Node<JoiningCluster> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }

        pub fn shutdown(self) -> Node<Shutdown> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }
    }

    impl Node<JoiningCluster> {
        pub fn joined(self) -> Node<InCluster> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }

        pub fn join_failed(self) -> Node<Ready> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }
    }

    impl Node<InCluster> {
        pub fn leave_cluster(self) -> Node<LeavingCluster> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }
    }

    impl Node<LeavingCluster> {
        pub fn left(self) -> Node<Ready> {
            Node {
                id: self.id,
                _state: PhantomData,
            }
        }
    }

    impl<S: TypeState> Node<S> {
        pub fn id(&self) -> u64 {
            self.id
        }

        pub fn state_name(&self) -> &'static str {
            S::STATE_NAME
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_state_machine() {
        use vm_lifecycle::*;

        let vm = Vm::new("test-vm".to_string(), VmConfig::default());
        assert_eq!(vm.state_name(), "Created");

        let vm = vm.configure();
        assert_eq!(vm.state_name(), "Configured");

        let vm = vm.start();
        assert_eq!(vm.state_name(), "Starting");

        let vm = vm.started();
        assert_eq!(vm.state_name(), "Running");

        let vm = vm.stop();
        assert_eq!(vm.state_name(), "Stopping");

        let vm = vm.stopped();
        assert_eq!(vm.state_name(), "Stopped");

        // This would not compile:
        // let vm = vm.start(); // Error: no method named `start` found for struct `Vm<Stopped>`
    }

    #[test]
    fn test_node_state_machine() {
        use node_lifecycle::*;

        let node = Node::new(1);
        assert_eq!(node.state_name(), "Uninitialized");

        let node = node.initialize();
        assert_eq!(node.state_name(), "Initializing");

        let node = node.initialized();
        assert_eq!(node.state_name(), "Ready");

        let node = node.join_cluster();
        assert_eq!(node.state_name(), "JoiningCluster");

        let node = node.joined();
        assert_eq!(node.state_name(), "InCluster");

        // Invalid transitions won't compile:
        // let node = node.initialize(); // Error: no method named `initialize`
    }
}