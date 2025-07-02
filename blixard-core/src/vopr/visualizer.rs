//! Visualization and reporting for fuzzer results
//! 
//! This module generates human-readable output similar to TigerBeetle's VOPR,
//! showing replica states, views, commit points, and interesting events.

use std::io::Write;
use std::collections::HashMap;
use crate::vopr::state_tracker::{StateSnapshot, NodeRole, Event, EventType};
use crate::vopr::operation_generator::Operation;

/// Configuration for visualization output
#[derive(Debug, Clone)]
pub struct VisualizerConfig {
    /// Show detailed node state
    pub show_node_details: bool,
    
    /// Show message flow
    pub show_messages: bool,
    
    /// Show resource usage
    pub show_resources: bool,
    
    /// Highlight anomalies
    pub highlight_anomalies: bool,
    
    /// Use color output
    pub use_color: bool,
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        Self {
            show_node_details: true,
            show_messages: true,
            show_resources: false,
            highlight_anomalies: true,
            use_color: true,
        }
    }
}

/// Main visualizer for fuzzer output
pub struct Visualizer {
    config: VisualizerConfig,
}

impl Visualizer {
    /// Create a new visualizer
    pub fn new(config: VisualizerConfig) -> Self {
        Self { config }
    }
    
    /// Generate a summary report
    pub fn generate_report<W: Write>(
        &self,
        writer: &mut W,
        seed: u64,
        operations: &[Operation],
        snapshots: &[StateSnapshot],
        events: &[Event],
        failure: Option<&str>,
    ) -> std::io::Result<()> {
        // Header
        writeln!(writer, "{}", "=".repeat(80))?;
        writeln!(writer, "VOPR Fuzzer Report")?;
        writeln!(writer, "{}", "=".repeat(80))?;
        writeln!(writer)?;
        
        // Summary
        writeln!(writer, "Seed: {}", seed)?;
        writeln!(writer, "Operations: {}", operations.len())?;
        writeln!(writer, "Snapshots: {}", snapshots.len())?;
        writeln!(writer, "Events: {}", events.len())?;
        
        if let Some(failure) = failure {
            writeln!(writer)?;
            writeln!(writer, "❌ FAILURE: {}", failure)?;
        }
        
        writeln!(writer)?;
        
        // Operation sequence
        self.write_operations(writer, operations)?;
        
        // State progression
        self.write_state_progression(writer, snapshots)?;
        
        // Interesting events
        self.write_events(writer, events)?;
        
        // Final state
        if let Some(final_state) = snapshots.last() {
            self.write_final_state(writer, final_state)?;
        }
        
        Ok(())
    }
    
    /// Write operation sequence
    fn write_operations<W: Write>(
        &self,
        writer: &mut W,
        operations: &[Operation],
    ) -> std::io::Result<()> {
        writeln!(writer, "Operation Sequence:")?;
        writeln!(writer, "{}", "-".repeat(40))?;
        
        for (i, op) in operations.iter().enumerate() {
            write!(writer, "{:4}: ", i)?;
            self.write_operation(writer, op)?;
            writeln!(writer)?;
        }
        
        writeln!(writer)?;
        Ok(())
    }
    
    /// Write a single operation
    fn write_operation<W: Write>(
        &self,
        writer: &mut W,
        op: &Operation,
    ) -> std::io::Result<()> {
        match op {
            Operation::StartNode { node_id } => {
                write!(writer, "StartNode({})", node_id)?;
            }
            Operation::StopNode { node_id } => {
                write!(writer, "StopNode({})", node_id)?;
            }
            Operation::RestartNode { node_id } => {
                write!(writer, "RestartNode({})", node_id)?;
            }
            Operation::ClientRequest { client_id, request_id, operation } => {
                write!(writer, "ClientRequest(client={}, req={}, op={:?})", 
                    client_id, request_id, operation)?;
            }
            Operation::NetworkPartition { partition_a, partition_b } => {
                write!(writer, "NetworkPartition({:?} | {:?})", partition_a, partition_b)?;
            }
            Operation::NetworkHeal => {
                write!(writer, "NetworkHeal")?;
            }
            Operation::ClockJump { node_id, delta_ms } => {
                write!(writer, "ClockJump(node={}, delta={}ms)", node_id, delta_ms)?;
            }
            Operation::ClockDrift { node_id, drift_rate_ppm } => {
                write!(writer, "ClockDrift(node={}, rate={}ppm)", node_id, drift_rate_ppm)?;
            }
            Operation::ByzantineNode { node_id, behavior } => {
                write!(writer, "ByzantineNode(node={}, behavior={:?})", node_id, behavior)?;
            }
            _ => {
                write!(writer, "{:?}", op)?;
            }
        }
        Ok(())
    }
    
    /// Write state progression
    fn write_state_progression<W: Write>(
        &self,
        writer: &mut W,
        snapshots: &[StateSnapshot],
    ) -> std::io::Result<()> {
        writeln!(writer, "State Progression:")?;
        writeln!(writer, "{}", "-".repeat(80))?;
        
        // Show compact view of state changes
        writeln!(writer, "Time  | Nodes | Leader | MaxView | MaxCommit | Partitions | Messages")?;
        writeln!(writer, "------|-------|--------|---------|-----------|------------|----------")?;
        
        for (i, snapshot) in snapshots.iter().enumerate() {
            // Find leader
            let leader = snapshot.nodes.values()
                .find(|n| n.role == NodeRole::Leader)
                .map(|n| n.id.to_string())
                .unwrap_or_else(|| "None".to_string());
            
            // Max view
            let max_view = snapshot.views.values().max().copied().unwrap_or(0);
            
            // Max commit
            let max_commit = snapshot.commit_points.values().max().copied().unwrap_or(0);
            
            // Node count
            let running_nodes = snapshot.nodes.values()
                .filter(|n| n.is_running)
                .count();
            
            writeln!(writer, "{:5} | {:5} | {:6} | {:7} | {:9} | {:10} | {:8}",
                i,
                running_nodes,
                leader,
                max_view,
                max_commit,
                snapshot.partitions.len(),
                snapshot.messages_in_flight,
            )?;
            
            // Highlight interesting moments
            if !snapshot.violations.is_empty() {
                for violation in &snapshot.violations {
                    writeln!(writer, "      ⚠️  {}", violation)?;
                }
            }
        }
        
        writeln!(writer)?;
        Ok(())
    }
    
    /// Write interesting events
    fn write_events<W: Write>(
        &self,
        writer: &mut W,
        events: &[Event],
    ) -> std::io::Result<()> {
        writeln!(writer, "Interesting Events:")?;
        writeln!(writer, "{}", "-".repeat(40))?;
        
        for event in events {
            match &event.event_type {
                EventType::RoleChange { from, to } => {
                    writeln!(writer, "  Node {} role change: {:?} -> {:?}",
                        event.node_id.unwrap_or(0), from, to)?;
                }
                EventType::ViewChange { from, to } => {
                    writeln!(writer, "  Node {} view change: {} -> {}",
                        event.node_id.unwrap_or(0), from, to)?;
                }
                EventType::InvariantViolation { invariant } => {
                    writeln!(writer, "  ❌ Invariant violation: {}", invariant)?;
                }
                EventType::AnomalyDetected { description } => {
                    writeln!(writer, "  ⚠️  Anomaly: {}", description)?;
                }
                _ => {
                    // Show other important events
                    if !event.details.is_empty() {
                        writeln!(writer, "  {:?}: {}", event.event_type, event.details)?;
                    }
                }
            }
        }
        
        writeln!(writer)?;
        Ok(())
    }
    
    /// Write final state details
    fn write_final_state<W: Write>(
        &self,
        writer: &mut W,
        state: &StateSnapshot,
    ) -> std::io::Result<()> {
        writeln!(writer, "Final State:")?;
        writeln!(writer, "{}", "-".repeat(40))?;
        
        // Node details
        writeln!(writer, "Nodes:")?;
        for (node_id, node_state) in &state.nodes {
            write!(writer, "  Node {}: ", node_id)?;
            
            if node_state.is_running {
                write!(writer, "{:?}", node_state.role)?;
                write!(writer, " | View: {}", state.views.get(node_id).unwrap_or(&0))?;
                write!(writer, " | Commit: {}", state.commit_points.get(node_id).unwrap_or(&0))?;
                write!(writer, " | Applied: {}", node_state.applied_index)?;
                
                if node_state.clock_skew_ms != 0 {
                    write!(writer, " | Skew: {}ms", node_state.clock_skew_ms)?;
                }
                
                if let Some(byzantine) = &node_state.byzantine {
                    write!(writer, " | Byzantine: {}", byzantine)?;
                }
            } else {
                write!(writer, "STOPPED")?;
            }
            
            writeln!(writer)?;
        }
        
        // Partitions
        if !state.partitions.is_empty() {
            writeln!(writer)?;
            writeln!(writer, "Network Partitions:")?;
            for (i, partition) in state.partitions.iter().enumerate() {
                writeln!(writer, "  Partition {}: {:?} | {:?}", 
                    i, partition.group_a, partition.group_b)?;
            }
        }
        
        // VMs
        if !state.vms.is_empty() {
            writeln!(writer)?;
            writeln!(writer, "VMs:")?;
            for (vm_id, vm_state) in &state.vms {
                writeln!(writer, "  {}: {:?} ({}cpu, {}MB) on node {:?}",
                    vm_id, vm_state.status, vm_state.cpu, vm_state.memory, vm_state.host_node)?;
            }
        }
        
        Ok(())
    }
    
    /// Generate a minimal reproducer script
    pub fn generate_reproducer<W: Write>(
        &self,
        writer: &mut W,
        seed: u64,
        operations: &[Operation],
    ) -> std::io::Result<()> {
        writeln!(writer, "// Minimal reproducer for bug found with seed {}", seed)?;
        writeln!(writer, "// Generated by VOPR fuzzer")?;
        writeln!(writer)?;
        
        writeln!(writer, "#[test]")?;
        writeln!(writer, "fn reproduce_bug() {{")?;
        writeln!(writer, "    let seed = {};", seed)?;
        writeln!(writer, "    let operations = vec![")?;
        
        for op in operations {
            write!(writer, "        ")?;
            self.write_operation_rust(writer, op)?;
            writeln!(writer, ",")?;
        }
        
        writeln!(writer, "    ];")?;
        writeln!(writer)?;
        writeln!(writer, "    // Run the operations")?;
        writeln!(writer, "    // TODO: Add test execution code")?;
        writeln!(writer, "}}")?;
        
        Ok(())
    }
    
    /// Write operation as Rust code
    fn write_operation_rust<W: Write>(
        &self,
        writer: &mut W,
        op: &Operation,
    ) -> std::io::Result<()> {
        match op {
            Operation::StartNode { node_id } => {
                write!(writer, "Operation::StartNode {{ node_id: {} }}", node_id)?;
            }
            Operation::StopNode { node_id } => {
                write!(writer, "Operation::StopNode {{ node_id: {} }}", node_id)?;
            }
            Operation::RestartNode { node_id } => {
                write!(writer, "Operation::RestartNode {{ node_id: {} }}", node_id)?;
            }
            Operation::ClockJump { node_id, delta_ms } => {
                write!(writer, "Operation::ClockJump {{ node_id: {}, delta_ms: {} }}", 
                    node_id, delta_ms)?;
            }
            _ => {
                write!(writer, "// TODO: {:?}", op)?;
            }
        }
        Ok(())
    }
}

/// Generate an ASCII timeline visualization
pub fn generate_timeline<W: Write>(
    writer: &mut W,
    snapshots: &[StateSnapshot],
) -> std::io::Result<()> {
    if snapshots.is_empty() {
        return Ok(());
    }
    
    // Find all nodes
    let mut all_nodes = std::collections::HashSet::new();
    for snapshot in snapshots {
        for node_id in snapshot.nodes.keys() {
            all_nodes.insert(*node_id);
        }
    }
    
    let mut nodes: Vec<_> = all_nodes.into_iter().collect();
    nodes.sort();
    
    // Header
    write!(writer, "Time |")?;
    for node_id in &nodes {
        write!(writer, " N{:2} |", node_id)?;
    }
    writeln!(writer)?;
    
    write!(writer, "-----|")?;
    for _ in &nodes {
        write!(writer, "-----|")?;
    }
    writeln!(writer)?;
    
    // Timeline
    for (i, snapshot) in snapshots.iter().enumerate() {
        write!(writer, "{:4} |", i)?;
        
        for node_id in &nodes {
            if let Some(node_state) = snapshot.nodes.get(node_id) {
                let symbol = if !node_state.is_running {
                    " X "
                } else {
                    match node_state.role {
                        NodeRole::Leader => " L ",
                        NodeRole::Candidate => " C ",
                        NodeRole::Follower => " F ",
                        NodeRole::Unknown => " ? ",
                    }
                };
                write!(writer, " {} |", symbol)?;
            } else {
                write!(writer, "    |")?;
            }
        }
        
        // Add events on this line
        if !snapshot.violations.is_empty() {
            write!(writer, " ❌")?;
        }
        
        writeln!(writer)?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_visualization() {
        let visualizer = Visualizer::new(VisualizerConfig::default());
        
        let operations = vec![
            Operation::StartNode { node_id: 1 },
            Operation::StartNode { node_id: 2 },
            Operation::StartNode { node_id: 3 },
        ];
        
        let mut output = Vec::new();
        visualizer.generate_report(
            &mut output,
            42,
            &operations,
            &[],
            &[],
            None,
        ).unwrap();
        
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Seed: 42"));
        assert!(output_str.contains("Operations: 3"));
    }
}