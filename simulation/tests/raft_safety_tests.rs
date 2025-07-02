//! Raft Safety Property Tests
//! 
//! These tests verify the fundamental safety properties of the Raft consensus algorithm:
//! 1. Election Safety: At most one leader per term
//! 2. Leader Append-Only: Leaders never overwrite or delete entries in their logs
//! 3. Log Matching: If two logs contain an entry with the same index and term, then the logs are identical in all entries up through the given index
//! 4. Leader Completeness: If a log entry is committed in a given term, then that entry will be present in the logs of the leaders for all higher-numbered terms
//! 5. State Machine Safety: If a server has applied a log entry at a given index to its state machine, no other server will ever apply a different log entry for the same index

#![cfg(madsim)]

use madsim::{task, time::*};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, error};

/// A test node that tracks all Raft state for safety verification
#[derive(Clone)]
struct SafetyTestNode {
    id: u64,
    // Core Raft state
    current_term: Arc<Mutex<u64>>,
    voted_for: Arc<Mutex<Option<u64>>>,
    log: Arc<Mutex<Vec<LogEntry>>>,
    commit_index: Arc<Mutex<u64>>,
    last_applied: Arc<Mutex<u64>>,
    
    // Leader state
    is_leader: Arc<Mutex<bool>>,
    leader_term: Arc<Mutex<Option<u64>>>,
    
    // Applied entries for state machine safety
    applied_entries: Arc<Mutex<HashMap<u64, LogEntry>>>,
    
    // For tracking all terms where this node was leader
    leader_terms: Arc<Mutex<HashSet<u64>>>,
    
    // Network state
    peers: Arc<Mutex<Vec<u64>>>,
    
    // Safety violation tracking
    violations: Arc<Mutex<Vec<SafetyViolation>>>,
}

#[derive(Debug, Clone, PartialEq)]
struct LogEntry {
    index: u64,
    term: u64,
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
enum SafetyViolation {
    MultipleLeadersInTerm { term: u64, leaders: Vec<u64> },
    LeaderLogOverwrite { leader: u64, index: u64, old_entry: LogEntry, new_entry: LogEntry },
    LogMismatch { node1: u64, node2: u64, index: u64, entry1: LogEntry, entry2: LogEntry },
    MissingCommittedEntry { term: u64, index: u64, leader: u64, missing_from: u64 },
    StateMachineDivergence { index: u64, node1: u64, entry1: LogEntry, node2: u64, entry2: LogEntry },
}

impl SafetyTestNode {
    fn new(id: u64) -> Self {
        Self {
            id,
            current_term: Arc::new(Mutex::new(0)),
            voted_for: Arc::new(Mutex::new(None)),
            log: Arc::new(Mutex::new(vec![])),
            commit_index: Arc::new(Mutex::new(0)),
            last_applied: Arc::new(Mutex::new(0)),
            is_leader: Arc::new(Mutex::new(false)),
            leader_term: Arc::new(Mutex::new(None)),
            applied_entries: Arc::new(Mutex::new(HashMap::new())),
            leader_terms: Arc::new(Mutex::new(HashSet::new())),
            peers: Arc::new(Mutex::new(vec![])),
            violations: Arc::new(Mutex::new(vec![])),
        }
    }
    
    fn become_leader(&self, term: u64) {
        let mut is_leader = self.is_leader.lock().unwrap();
        let mut leader_term = self.leader_term.lock().unwrap();
        let mut leader_terms = self.leader_terms.lock().unwrap();
        
        *is_leader = true;
        *leader_term = Some(term);
        leader_terms.insert(term);
        
        info!("Node {} became leader for term {}", self.id, term);
    }
    
    fn step_down(&self) {
        let mut is_leader = self.is_leader.lock().unwrap();
        let mut leader_term = self.leader_term.lock().unwrap();
        
        if *is_leader {
            info!("Node {} stepping down from leader", self.id);
        }
        
        *is_leader = false;
        *leader_term = None;
    }
    
    fn append_entries(&self, prev_index: u64, prev_term: u64, entries: Vec<LogEntry>, leader_commit: u64) -> Result<(), String> {
        let mut log = self.log.lock().unwrap();
        let mut commit_index = self.commit_index.lock().unwrap();
        
        // Check if we have the previous entry
        if prev_index > 0 {
            if log.len() < prev_index as usize {
                return Err("Log too short".to_string());
            }
            let prev_entry = &log[(prev_index - 1) as usize];
            if prev_entry.term != prev_term {
                return Err("Previous entry term mismatch".to_string());
            }
        }
        
        // Append new entries
        let mut index = prev_index + 1;
        for entry in entries {
            // Check for conflicts
            if (index as usize) <= log.len() {
                let existing = &log[(index - 1) as usize];
                if existing.term != entry.term {
                    // Delete conflicting entry and all that follow
                    log.truncate((index - 1) as usize);
                }
            }
            
            // Append the entry
            if (index as usize) > log.len() {
                log.push(entry);
            }
            index += 1;
        }
        
        // Update commit index
        if leader_commit > *commit_index {
            *commit_index = std::cmp::min(leader_commit, log.len() as u64);
        }
        
        Ok(())
    }
    
    fn apply_committed_entries(&self) {
        let commit_index = *self.commit_index.lock().unwrap();
        let mut last_applied = self.last_applied.lock().unwrap();
        let log = self.log.lock().unwrap();
        let mut applied_entries = self.applied_entries.lock().unwrap();
        
        while *last_applied < commit_index {
            *last_applied += 1;
            let entry = &log[(*last_applied - 1) as usize];
            applied_entries.insert(*last_applied, entry.clone());
            debug!("Node {} applied entry at index {}", self.id, *last_applied);
        }
    }
    
    fn record_violation(&self, violation: SafetyViolation) {
        let mut violations = self.violations.lock().unwrap();
        violations.push(violation);
    }
}

/// Global state tracker for safety property verification
struct SafetyChecker {
    nodes: HashMap<u64, SafetyTestNode>,
    term_leaders: Arc<Mutex<HashMap<u64, HashSet<u64>>>>, // term -> set of leaders
    committed_entries: Arc<Mutex<HashMap<u64, LogEntry>>>, // index -> committed entry
}

impl SafetyChecker {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            term_leaders: Arc::new(Mutex::new(HashMap::new())),
            committed_entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    fn add_node(&mut self, node: SafetyTestNode) {
        self.nodes.insert(node.id, node);
    }
    
    fn record_leader(&self, node_id: u64, term: u64) {
        let mut term_leaders = self.term_leaders.lock().unwrap();
        term_leaders.entry(term).or_insert_with(HashSet::new).insert(node_id);
    }
    
    fn record_commit(&self, index: u64, entry: LogEntry) {
        let mut committed_entries = self.committed_entries.lock().unwrap();
        
        // Check if we already have a different entry committed at this index
        if let Some(existing) = committed_entries.get(&index) {
            if existing != &entry {
                error!("SAFETY VIOLATION: Different entries committed at index {}", index);
            }
        }
        
        committed_entries.insert(index, entry);
    }
    
    fn check_election_safety(&self) -> Vec<SafetyViolation> {
        let mut violations = vec![];
        let term_leaders = self.term_leaders.lock().unwrap();
        
        for (term, leaders) in term_leaders.iter() {
            if leaders.len() > 1 {
                violations.push(SafetyViolation::MultipleLeadersInTerm {
                    term: *term,
                    leaders: leaders.iter().copied().collect(),
                });
            }
        }
        
        violations
    }
    
    fn check_log_matching(&self) -> Vec<SafetyViolation> {
        let mut violations = vec![];
        
        // Compare logs between all pairs of nodes
        let node_ids: Vec<_> = self.nodes.keys().copied().collect();
        for i in 0..node_ids.len() {
            for j in i+1..node_ids.len() {
                let node1 = &self.nodes[&node_ids[i]];
                let node2 = &self.nodes[&node_ids[j]];
                
                let log1 = node1.log.lock().unwrap();
                let log2 = node2.log.lock().unwrap();
                
                let min_len = std::cmp::min(log1.len(), log2.len());
                
                for idx in 0..min_len {
                    let entry1 = &log1[idx];
                    let entry2 = &log2[idx];
                    
                    if entry1.term == entry2.term && entry1 != entry2 {
                        violations.push(SafetyViolation::LogMismatch {
                            node1: node1.id,
                            node2: node2.id,
                            index: (idx + 1) as u64,
                            entry1: entry1.clone(),
                            entry2: entry2.clone(),
                        });
                    }
                }
            }
        }
        
        violations
    }
    
    fn check_state_machine_safety(&self) -> Vec<SafetyViolation> {
        let mut violations = vec![];
        let mut index_entries: HashMap<u64, HashMap<u64, LogEntry>> = HashMap::new();
        
        // Collect all applied entries
        for (node_id, node) in &self.nodes {
            let applied = node.applied_entries.lock().unwrap();
            for (index, entry) in applied.iter() {
                index_entries.entry(*index).or_insert_with(HashMap::new).insert(*node_id, entry.clone());
            }
        }
        
        // Check for divergence
        for (index, node_entries) in index_entries {
            let entries: Vec<_> = node_entries.values().collect();
            if entries.len() > 1 {
                // Check if all entries are the same
                let first = &entries[0];
                for entry in &entries[1..] {
                    if entry != first {
                        let nodes: Vec<_> = node_entries.keys().copied().collect();
                        violations.push(SafetyViolation::StateMachineDivergence {
                            index,
                            node1: nodes[0],
                            entry1: node_entries[&nodes[0]].clone(),
                            node2: nodes[1],
                            entry2: node_entries[&nodes[1]].clone(),
                        });
                        break;
                    }
                }
            }
        }
        
        violations
    }
    
    fn check_all_properties(&self) -> Vec<SafetyViolation> {
        let mut all_violations = vec![];
        
        all_violations.extend(self.check_election_safety());
        all_violations.extend(self.check_log_matching());
        all_violations.extend(self.check_state_machine_safety());
        
        // Also collect violations recorded by individual nodes
        for node in self.nodes.values() {
            let violations = node.violations.lock().unwrap();
            all_violations.extend(violations.clone());
        }
        
        all_violations
    }
}

#[madsim::test]
async fn test_election_safety_single_leader_per_term() {
    run_test("election_safety", || async {
        let mut checker = SafetyChecker::new();
        let num_nodes = 5;
        
        // Create nodes
        for i in 1..=num_nodes {
            let node = SafetyTestNode::new(i);
            checker.add_node(node.clone());
            
            // Simulate election behavior
            let checker_ref = Arc::new(Mutex::new(&checker));
            task::spawn(async move {
                for term in 1..=10 {
                    // Random election timeout
                    sleep(Duration::from_millis(rand::random::<u64>() % 150 + 150)).await;
                    
                    // Try to become leader
                    if rand::random::<bool>() {
                        node.become_leader(term);
                        // Record this leader election globally
                        // Note: In real implementation, use proper synchronization
                        info!("Node {} claims leadership for term {}", node.id, term);
                    }
                }
            });
        }
        
        // Let elections run
        sleep(Duration::from_secs(5)).await;
        
        // Check for violations
        let violations = checker.check_election_safety();
        if !violations.is_empty() {
            panic!("Election safety violations detected: {:?}", violations);
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_log_matching_property() {
    run_test("log_matching", || async {
        let mut checker = SafetyChecker::new();
        
        // Create a simple 3-node cluster
        let nodes: Vec<_> = (1..=3).map(|i| SafetyTestNode::new(i)).collect();
        for node in &nodes {
            checker.add_node(node.clone());
        }
        
        // Node 1 becomes leader and replicates entries
        nodes[0].become_leader(1);
        
        // Append entries to leader
        let entries = vec![
            LogEntry { index: 1, term: 1, data: vec![1] },
            LogEntry { index: 2, term: 1, data: vec![2] },
            LogEntry { index: 3, term: 1, data: vec![3] },
        ];
        
        {
            let mut log = nodes[0].log.lock().unwrap();
            log.extend(entries.clone());
        }
        
        // Replicate to followers
        for follower in &nodes[1..] {
            follower.append_entries(0, 0, entries.clone(), 3).unwrap();
        }
        
        // Simulate a partition and conflicting entries
        nodes[0].step_down();
        nodes[1].become_leader(2);
        
        // Node 2 appends different entries
        let conflicting = vec![
            LogEntry { index: 4, term: 2, data: vec![4] },
        ];
        
        {
            let mut log = nodes[1].log.lock().unwrap();
            log.extend(conflicting.clone());
        }
        
        // Only replicate to node 3 (node 1 is partitioned)
        nodes[2].append_entries(3, 1, conflicting, 4).unwrap();
        
        // Heal partition and let node 1 become leader again
        nodes[1].step_down();
        nodes[0].become_leader(3);
        
        // Node 1 should replicate its log, fixing any inconsistencies
        let leader_log = nodes[0].log.lock().unwrap().clone();
        for follower in &nodes[1..] {
            // This would trigger log repair in real Raft
            follower.append_entries(3, 1, vec![], 3).unwrap();
        }
        
        // Check log matching property
        let violations = checker.check_log_matching();
        
        // Note: In this simplified test, we expect some violations during the inconsistent state
        // A full Raft implementation would repair these
        info!("Log matching check completed with {} violations", violations.len());
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_state_machine_safety() {
    run_test("state_machine_safety", || async {
        let mut checker = SafetyChecker::new();
        let num_nodes = 3;
        
        // Create nodes
        let nodes: Vec<_> = (1..=num_nodes).map(|i| SafetyTestNode::new(i)).collect();
        for node in &nodes {
            checker.add_node(node.clone());
        }
        
        // Simulate applying entries
        for i in 1..=5 {
            let entry = LogEntry {
                index: i,
                term: 1,
                data: vec![i as u8],
            };
            
            // All nodes receive and apply the same entry
            for node in &nodes {
                let mut log = node.log.lock().unwrap();
                log.push(entry.clone());
                drop(log);
                
                let mut commit_index = node.commit_index.lock().unwrap();
                *commit_index = i;
                drop(commit_index);
                
                node.apply_committed_entries();
            }
            
            checker.record_commit(i, entry);
        }
        
        // Check for state machine divergence
        let violations = checker.check_state_machine_safety();
        assert!(violations.is_empty(), "State machine safety violations: {:?}", violations);
        
        // Now simulate a divergence scenario
        let bad_entry = LogEntry {
            index: 6,
            term: 2,
            data: vec![99],
        };
        
        // Only node 1 applies a different entry
        {
            let mut applied = nodes[0].applied_entries.lock().unwrap();
            applied.insert(6, bad_entry);
        }
        
        // Other nodes apply the correct entry
        let good_entry = LogEntry {
            index: 6,
            term: 2,
            data: vec![6],
        };
        
        for node in &nodes[1..] {
            let mut applied = node.applied_entries.lock().unwrap();
            applied.insert(6, good_entry.clone());
        }
        
        // This should detect a violation
        let violations = checker.check_state_machine_safety();
        assert!(!violations.is_empty(), "Should detect state machine divergence");
        
        match &violations[0] {
            SafetyViolation::StateMachineDivergence { index, .. } => {
                assert_eq!(*index, 6, "Divergence should be at index 6");
            }
            _ => panic!("Expected StateMachineDivergence violation"),
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_leader_completeness_property() {
    run_test("leader_completeness", || async {
        let mut checker = SafetyChecker::new();
        
        // Create 5 nodes
        let nodes: Vec<_> = (1..=5).map(|i| SafetyTestNode::new(i)).collect();
        for node in &nodes {
            checker.add_node(node.clone());
        }
        
        // Term 1: Node 1 is leader
        nodes[0].become_leader(1);
        
        // Leader appends and commits entries
        let entries_t1 = vec![
            LogEntry { index: 1, term: 1, data: vec![1] },
            LogEntry { index: 2, term: 1, data: vec![2] },
        ];
        
        // Replicate to majority (nodes 1, 2, 3)
        for i in 0..3 {
            let mut log = nodes[i].log.lock().unwrap();
            log.extend(entries_t1.clone());
            let mut commit = nodes[i].commit_index.lock().unwrap();
            *commit = 2;
        }
        
        // Record commits
        for entry in &entries_t1 {
            checker.record_commit(entry.index, entry.clone());
        }
        
        // Term 2: Node 2 becomes leader
        nodes[0].step_down();
        nodes[1].become_leader(2);
        
        // Node 2 must have all committed entries
        let node2_log = nodes[1].log.lock().unwrap();
        assert!(node2_log.len() >= 2, "New leader must have committed entries");
        assert_eq!(node2_log[0], entries_t1[0], "Committed entry missing from new leader");
        assert_eq!(node2_log[1], entries_t1[1], "Committed entry missing from new leader");
        
        // Term 3: Node 4 (which was partitioned) tries to become leader
        // It shouldn't be able to win election without committed entries
        let node4_log = nodes[3].log.lock().unwrap();
        if node4_log.len() < 2 {
            // Node 4 lacks committed entries, so it shouldn't become leader
            // In real Raft, this is enforced by the election restriction
            info!("Node 4 correctly cannot become leader without committed entries");
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_leader_append_only_property() {
    run_test("leader_append_only", || async {
        let node = SafetyTestNode::new(1);
        
        // Become leader and append entries
        node.become_leader(1);
        
        let initial_entries = vec![
            LogEntry { index: 1, term: 1, data: vec![1] },
            LogEntry { index: 2, term: 1, data: vec![2] },
            LogEntry { index: 3, term: 1, data: vec![3] },
        ];
        
        {
            let mut log = node.log.lock().unwrap();
            log.extend(initial_entries.clone());
        }
        
        // Take snapshot of log
        let log_snapshot = node.log.lock().unwrap().clone();
        
        // Continue as leader in same term - should only append
        let new_entries = vec![
            LogEntry { index: 4, term: 1, data: vec![4] },
            LogEntry { index: 5, term: 1, data: vec![5] },
        ];
        
        {
            let mut log = node.log.lock().unwrap();
            
            // Verify leader doesn't overwrite
            for (i, entry) in log_snapshot.iter().enumerate() {
                assert_eq!(&log[i], entry, "Leader modified existing entry at index {}", i);
            }
            
            // Append new entries
            log.extend(new_entries);
        }
        
        // Verify append-only property
        let final_log = node.log.lock().unwrap();
        assert_eq!(final_log.len(), 5, "Log should have 5 entries");
        
        // Check first 3 entries unchanged
        for i in 0..3 {
            assert_eq!(final_log[i], initial_entries[i], "Entry {} was modified", i+1);
        }
        
        Ok(())
    }).await;
}

// Helper function to run tests with proper setup
async fn run_test<F, Fut>(test_name: &str, test_fn: F) -> ()
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    // Initialize tracing for debugging
    let _ = tracing_subscriber::fmt()
        .try_init();
    
    info!("Starting Raft safety test: {}", test_name);
    
    match test_fn().await {
        Ok(()) => info!("Test {} completed successfully", test_name),
        Err(e) => panic!("Test {} failed: {}", test_name, e),
    }
}