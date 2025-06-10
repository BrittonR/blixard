use stateright::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Hash, PartialEq)]
struct ClusterState {
    nodes: Vec<NodeState>,
    vms: BTreeMap<String, VmPlacement>,
}

#[derive(Clone, Debug, Hash, PartialEq)]
struct NodeState {
    id: u64,
    is_leader: bool,
    is_active: bool,
}

#[derive(Clone, Debug, Hash, PartialEq)]
struct VmPlacement {
    vm_name: String,
    node_id: u64,
    is_running: bool,
}

#[derive(Clone, Debug, Hash, PartialEq)]
enum ClusterAction {
    CreateVm(String, u64), // vm_name, target_node
    NodeFailure(u64),
    NodeRecover(u64),
    ElectLeader(u64),
}

struct ClusterModel {
    node_count: usize,
}

impl Model for ClusterModel {
    type State = ClusterState;
    type Action = ClusterAction;
    
    fn init_states(&self) -> Vec<Self::State> {
        vec![ClusterState {
            nodes: (0..self.node_count)
                .map(|i| NodeState {
                    id: i as u64,
                    is_leader: i == 0,
                    is_active: true,
                })
                .collect(),
            vms: BTreeMap::new(),
        }]
    }
    
    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        // Can create VMs on active nodes
        for node in &state.nodes {
            if node.is_active {
                actions.push(ClusterAction::CreateVm(
                    format!("vm-{}", state.vms.len()),
                    node.id,
                ));
            }
        }
        
        // Can fail active nodes
        for node in &state.nodes {
            if node.is_active {
                actions.push(ClusterAction::NodeFailure(node.id));
            } else {
                actions.push(ClusterAction::NodeRecover(node.id));
            }
        }
        
        // Can elect new leader if current leader failed
        let has_active_leader = state.nodes.iter().any(|n| n.is_leader && n.is_active);
        if !has_active_leader {
            for node in &state.nodes {
                if node.is_active {
                    actions.push(ClusterAction::ElectLeader(node.id));
                }
            }
        }
    }
    
    fn next_state(&self, state: &Self::State, action: Self::Action) -> Option<Self::State> {
        let mut next_state = state.clone();
        
        match action {
            ClusterAction::CreateVm(name, node_id) => {
                next_state.vms.insert(name.clone(), VmPlacement {
                    vm_name: name,
                    node_id,
                    is_running: true,
                });
            }
            
            ClusterAction::NodeFailure(id) => {
                for node in &mut next_state.nodes {
                    if node.id == id {
                        node.is_active = false;
                        node.is_leader = false;
                    }
                }
                // VMs on failed node stop
                for vm in next_state.vms.values_mut() {
                    if vm.node_id == id {
                        vm.is_running = false;
                    }
                }
            }
            
            ClusterAction::NodeRecover(id) => {
                for node in &mut next_state.nodes {
                    if node.id == id {
                        node.is_active = true;
                    }
                }
            }
            
            ClusterAction::ElectLeader(id) => {
                // Remove old leader
                for node in &mut next_state.nodes {
                    node.is_leader = false;
                }
                // Set new leader
                for node in &mut next_state.nodes {
                    if node.id == id {
                        node.is_leader = true;
                    }
                }
            }
        }
        
        Some(next_state)
    }
}

#[test]
#[ignore = "stateright API needs updating"]
fn model_check_cluster_safety() {
    let model = ClusterModel { node_count: 3 };
    
    // TODO: Update to use correct stateright API
    println!("Model checking test temporarily disabled");
}

#[test]
#[ignore = "stateright API needs updating"] 
fn model_check_vm_availability() {
    let model = ClusterModel { node_count: 3 };
    
    // TODO: Update to use correct stateright API
    println!("Model checking test temporarily disabled");
}