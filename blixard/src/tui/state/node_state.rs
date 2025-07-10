//! Node-related state management

use crate::tui::types::node::{NodeInfo, NodeFilter};
use crate::tui::forms::node::{CreateNodeForm, EditNodeForm};
use ratatui::widgets::{ListState, TableState};
use std::collections::HashMap;

#[derive(Debug)]
pub struct NodeState {
    // Node data
    pub nodes: Vec<NodeInfo>,
    pub filtered_nodes: Vec<NodeInfo>,
    
    // UI state
    pub node_list_state: ListState,
    pub node_table_state: TableState,
    pub selected_node: Option<u64>,
    
    // Daemon processes
    pub daemon_processes: HashMap<u64, u32>, // node_id -> PID
    pub batch_node_count: String,
    
    // Forms
    pub create_node_form: CreateNodeForm,
    pub edit_node_form: EditNodeForm,
    
    // Filtering
    pub node_filter: NodeFilter,
}

impl NodeState {
    pub fn new() -> Self {
        let mut node_list_state = ListState::default();
        node_list_state.select(Some(0));
        
        let mut node_table_state = TableState::default();
        node_table_state.select(Some(0));

        Self {
            nodes: Vec::new(),
            filtered_nodes: Vec::new(),
            node_list_state,
            node_table_state,
            selected_node: None,
            daemon_processes: HashMap::new(),
            batch_node_count: "3".to_string(),
            create_node_form: CreateNodeForm::new(),
            edit_node_form: EditNodeForm::from_node_info(&NodeInfo {
                id: 0,
                address: String::new(),
                status: crate::tui::types::node::NodeStatus::Healthy,
                role: crate::tui::types::node::NodeRole::Follower,
                cpu_usage: 0.0,
                memory_usage: 0.0,
                vm_count: 0,
                last_seen: None,
                version: None,
            }),
            node_filter: NodeFilter::All,
        }
    }

    pub fn apply_filter(&mut self) {
        self.filtered_nodes = match &self.node_filter {
            NodeFilter::All => self.nodes.clone(),
            NodeFilter::Healthy => self.nodes.iter()
                .filter(|n| matches!(n.status, crate::tui::types::node::NodeStatus::Healthy))
                .cloned()
                .collect(),
            NodeFilter::Unhealthy => self.nodes.iter()
                .filter(|n| !matches!(n.status, crate::tui::types::node::NodeStatus::Healthy))
                .cloned()
                .collect(),
            NodeFilter::Leader => self.nodes.iter()
                .filter(|n| matches!(n.role, crate::tui::types::node::NodeRole::Leader))
                .cloned()
                .collect(),
            NodeFilter::Followers => self.nodes.iter()
                .filter(|n| matches!(n.role, crate::tui::types::node::NodeRole::Follower))
                .cloned()
                .collect(),
        };
    }

    pub fn get_displayed_nodes(&self) -> &[NodeInfo] {
        if self.filtered_nodes.is_empty() {
            &self.nodes
        } else {
            &self.filtered_nodes
        }
    }
}