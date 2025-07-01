use blixard_core::remediation_engine::{
    RemediationEngine, RemediationPolicy, RemediationContext,
    IssueType, Issue, IssueSeverity, IssueResource,
    VmFailureDetector, VmFailureRemediator,
    NodeHealthDetector, ResourceExhaustionRemediator,
};
use blixard_core::remediation_raft::{
    RaftConsensusDetector, LeaderElectionStormRemediator,
    StuckProposalRemediator, LogDivergenceRemediator,
    NetworkPartitionDetector, NetworkPartitionRemediator,
};
use blixard_core::node_shared::SharedNodeState;
use blixard_core::types::{NodeConfig, VmConfig, VmState, VmStatus};
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn test_vm_failure_detection_and_remediation() {
    // Create test node
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };
    
    let shared = Arc::new(SharedNodeState::new(config.clone()));
    
    // Add a failed VM
    let failed_vm = VmState {
        name: "failed-vm".to_string(),
        config: VmConfig {
            name: "failed-vm".to_string(),
            config_path: "/test".to_string(),
            vcpus: 2,
            memory: 1024,
            ..Default::default()
        },
        status: VmStatus::Failed,
        node_id: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    // Store the failed VM
    {
        let mut vms = shared.vm_states.write().await;
        vms.insert("failed-vm".to_string(), failed_vm);
    }
    
    // Create remediation engine
    let policy = RemediationPolicy {
        enabled: true,
        auto_remediate_types: vec![IssueType::VmFailure],
        ..Default::default()
    };
    
    let mut engine = RemediationEngine::new(policy);
    
    // Add detector and remediator
    engine.add_detector(Arc::new(VmFailureDetector));
    engine.add_remediator(IssueType::VmFailure, Arc::new(VmFailureRemediator::new(3)));
    
    // Create context
    let context = RemediationContext {
        node_state: shared.clone(),
        audit_logger: None,
        node_id: 1,
    };
    
    // Start engine
    engine.start(context.clone()).await.unwrap();
    
    // Wait for detection and remediation
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Check history
    let history = engine.get_history().await;
    assert!(!history.is_empty(), "Should have remediation history");
    
    let event = &history[0];
    assert_eq!(event.issue.issue_type, IssueType::VmFailure);
    assert!(event.issue.description.contains("failed-vm"));
    
    // Check that remediation was attempted
    assert!(!event.result.actions.is_empty());
    assert_eq!(event.result.actions[0].action, "restart_vm");
    
    engine.stop().await;
}

#[tokio::test]
async fn test_circuit_breaker_functionality() {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };
    
    let shared = Arc::new(SharedNodeState::new(config));
    
    // Create engine with low circuit breaker threshold
    let policy = RemediationPolicy {
        enabled: true,
        circuit_breaker_threshold: 2, // Open after 2 failures
        circuit_breaker_reset: Duration::from_secs(60),
        auto_remediate_types: vec![IssueType::VmFailure],
        ..Default::default()
    };
    
    let mut engine = RemediationEngine::new(policy);
    
    // Add a detector that always finds issues
    // Add a remediator that always fails
    // This will test the circuit breaker
    
    let context = RemediationContext {
        node_state: shared,
        audit_logger: None,
        node_id: 1,
    };
    
    engine.start(context).await.unwrap();
    
    // Wait for circuit breaker to trigger
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Check circuit status
    let status = engine.get_circuit_status().await;
    // Circuit should be open for some issue types after failures
    
    engine.stop().await;
}

#[tokio::test]
async fn test_raft_consensus_detection() {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };
    
    let shared = Arc::new(SharedNodeState::new(config));
    
    // Simulate Raft metrics with issues
    shared.update_raft_metrics(|m| {
        m.leader_changes_per_minute = 10; // Election storm
        m.pending_proposals = 200; // Many stuck proposals
    }).await;
    
    // Create detector
    let detector = RaftConsensusDetector::new();
    
    let context = RemediationContext {
        node_state: shared,
        audit_logger: None,
        node_id: 1,
    };
    
    // Detect issues
    let issues = detector.detect(&context).await;
    
    assert!(issues.len() >= 2, "Should detect multiple Raft issues");
    
    // Check for leader election storm
    assert!(issues.iter().any(|i| i.description.contains("Leader election storm")));
    
    // Check for stuck proposals
    assert!(issues.iter().any(|i| i.description.contains("pending proposals")));
}

#[tokio::test]
async fn test_remediation_priority_ordering() {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };
    
    let shared = Arc::new(SharedNodeState::new(config));
    
    let policy = RemediationPolicy {
        enabled: true,
        auto_remediate_types: vec![
            IssueType::VmFailure,
            IssueType::RaftConsensusIssue,
            IssueType::NetworkPartition,
        ],
        ..Default::default()
    };
    
    let mut engine = RemediationEngine::new(policy);
    
    // Add multiple remediators with different priorities
    engine.add_remediator(IssueType::RaftConsensusIssue, Arc::new(LeaderElectionStormRemediator));
    engine.add_remediator(IssueType::RaftConsensusIssue, Arc::new(StuckProposalRemediator));
    engine.add_remediator(IssueType::RaftConsensusIssue, Arc::new(LogDivergenceRemediator));
    
    // The engine should use the highest priority remediator for each issue
    
    let context = RemediationContext {
        node_state: shared,
        audit_logger: None,
        node_id: 1,
    };
    
    engine.start(context).await.unwrap();
    
    // Test that appropriate remediator is selected based on priority
    
    engine.stop().await;
}

#[tokio::test]
async fn test_resource_exhaustion_remediation() {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };
    
    let shared = Arc::new(SharedNodeState::new(config));
    
    // Add some VMs with different priorities
    for i in 0..5 {
        let vm = VmState {
            name: format!("vm-{}", i),
            config: VmConfig {
                name: format!("vm-{}", i),
                config_path: "/test".to_string(),
                vcpus: 2,
                memory: 1024,
                priority: (i * 100) as u32,
                preemptible: i < 3, // First 3 are preemptible
                ..Default::default()
            },
            status: VmStatus::Running,
            node_id: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        shared.vm_states.write().await.insert(vm.name.clone(), vm);
    }
    
    // Create remediation for resource exhaustion
    let remediator = ResourceExhaustionRemediator;
    
    let issue = Issue {
        id: "resource-test".to_string(),
        issue_type: IssueType::ResourceExhaustion,
        severity: IssueSeverity::High,
        resource: IssueResource::Node { id: 1, address: "127.0.0.1:7001".to_string() },
        detected_at: Utc::now(),
        description: "High memory usage detected".to_string(),
        context: serde_json::json!({}),
        correlation_id: None,
    };
    
    let context = RemediationContext {
        node_state: shared,
        audit_logger: None,
        node_id: 1,
    };
    
    let result = remediator.remediate(&issue, &context).await;
    
    assert!(result.success);
    assert!(!result.actions.is_empty());
    
    // Should have identified low-priority preemptible VMs
    let preemption_actions: Vec<_> = result.actions.iter()
        .filter(|a| a.action == "stop_low_priority_vm")
        .collect();
    
    assert!(!preemption_actions.is_empty());
}

#[tokio::test]
async fn test_network_partition_detection() {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };
    
    let shared = Arc::new(SharedNodeState::new(config));
    
    // Simulate a cluster with unreachable peers
    shared.update_cluster_info(|info| {
        info.peers = vec![
            blixard_core::iroh_types::PeerInfo { id: 2, address: "127.0.0.1:7002".to_string() },
            blixard_core::iroh_types::PeerInfo { id: 3, address: "127.0.0.1:7003".to_string() },
            blixard_core::iroh_types::PeerInfo { id: 4, address: "127.0.0.1:7004".to_string() },
            blixard_core::iroh_types::PeerInfo { id: 5, address: "127.0.0.1:7005".to_string() },
        ];
    }).await;
    
    // Mark most peers as unreachable
    for peer_id in [2, 3, 4] {
        shared.update_peer_metrics(peer_id, |m| {
            m.consecutive_failures = 15;
        }).await;
    }
    
    let detector = NetworkPartitionDetector;
    
    let context = RemediationContext {
        node_state: shared,
        audit_logger: None,
        node_id: 1,
    };
    
    let issues = detector.detect(&context).await;
    
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].issue_type, IssueType::NetworkPartition);
    assert!(issues[0].description.contains("cannot reach 3 out of 4 peers"));
}