use blixard_core::raft_manager::WorkerCapabilities;
use blixard_core::storage::{
    NODE_TOPOLOGY_TABLE, VM_STATE_TABLE, WORKER_STATUS_TABLE, WORKER_TABLE,
};
use blixard_core::types::{LocalityPreference, NodeTopology, VmConfig};
use blixard_core::vm_scheduler::{PlacementStrategy, VmScheduler};
use redb::Database;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_locality_aware_placement_strategy() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(&db_path).unwrap());

    // Initialize tables
    {
        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(WORKER_TABLE).unwrap();
        write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        write_txn.open_table(NODE_TOPOLOGY_TABLE).unwrap();
        write_txn.open_table(VM_STATE_TABLE).unwrap();
        write_txn.commit().unwrap();
    }

    let scheduler = VmScheduler::new(db.clone());

    // Register workers in different datacenters
    {
        let write_txn = db.begin_write().unwrap();
        {
            let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
            let mut topology_table = write_txn.open_table(NODE_TOPOLOGY_TABLE).unwrap();

            // Create 3 nodes in different configurations
            for (node_id, dc, zone, cpu, mem) in [
                (1u64, "us-east-1", "zone-a", 8u32, 16384u64),
                (2u64, "us-east-1", "zone-b", 4u32, 8192u64),
                (3u64, "us-west-1", "zone-a", 16u32, 32768u64),
            ] {
                let capabilities = WorkerCapabilities {
                    cpu_cores: cpu,
                    memory_mb: mem,
                    disk_gb: 100,
                    features: vec!["microvm".to_string()],
                };
                let topology = NodeTopology {
                    datacenter: dc.to_string(),
                    zone: zone.to_string(),
                    rack: format!("rack-{}", node_id),
                };

                let worker_data =
                    bincode::serialize(&(format!("node{}", node_id), capabilities)).unwrap();
                worker_table
                    .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())
                    .unwrap();
                status_table
                    .insert(node_id.to_le_bytes().as_slice(), [0u8].as_slice())
                    .unwrap();
                topology_table
                    .insert(
                        node_id.to_le_bytes().as_slice(),
                        bincode::serialize(&topology).unwrap().as_slice(),
                    )
                    .unwrap();
            }
        }
        write_txn.commit().unwrap();
    }

    // Test 1: LocalityAware with strict enforcement
    let vm_config = VmConfig {
        name: "locality-strict-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        locality_preference: LocalityPreference {
            preferred_datacenter: Some("us-east-1".to_string()),
            preferred_zone: Some("zone-a".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let strategy = PlacementStrategy::LocalityAware {
        base_strategy: Box::new(PlacementStrategy::MostAvailable),
        strict: true,
    };

    let decision = scheduler
        .schedule_vm_placement(&vm_config, strategy)
        .await
        .unwrap();
    // Should select node 1 (us-east-1, zone-a)
    assert_eq!(decision.target_node_id, 1);

    // Test 2: LocalityAware non-strict with impossible preference
    let vm_config2 = VmConfig {
        name: "locality-nonstrict-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        locality_preference: LocalityPreference {
            preferred_datacenter: Some("eu-west-1".to_string()), // Doesn't exist
            ..Default::default()
        },
        ..Default::default()
    };

    let strategy2 = PlacementStrategy::LocalityAware {
        base_strategy: Box::new(PlacementStrategy::MostAvailable),
        strict: false,
    };

    let decision2 = scheduler
        .schedule_vm_placement(&vm_config2, strategy2)
        .await
        .unwrap();
    // Should select any available node (non-strict)
    assert!(decision2.target_node_id >= 1 && decision2.target_node_id <= 3);

    // Test 3: LocalityAware strict with impossible preference should fail
    let strategy3 = PlacementStrategy::LocalityAware {
        base_strategy: Box::new(PlacementStrategy::MostAvailable),
        strict: true,
    };

    let result3 = scheduler
        .schedule_vm_placement(&vm_config2, strategy3)
        .await;
    assert!(result3.is_err());
    assert!(result3
        .unwrap_err()
        .to_string()
        .contains("Strict locality requirement not met"));
}

#[tokio::test]
async fn test_spread_across_failure_domains_strategy() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(&db_path).unwrap());

    // Initialize tables
    {
        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(WORKER_TABLE).unwrap();
        write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        write_txn.open_table(NODE_TOPOLOGY_TABLE).unwrap();
        write_txn.open_table(VM_STATE_TABLE).unwrap();
        write_txn.commit().unwrap();
    }

    let scheduler = VmScheduler::new(db.clone());

    // Register workers with VMs already placed
    {
        let write_txn = db.begin_write().unwrap();
        {
            let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
            let mut topology_table = write_txn.open_table(NODE_TOPOLOGY_TABLE).unwrap();
            let mut vm_table = write_txn.open_table(VM_STATE_TABLE).unwrap();

            // Create nodes in 3 zones with different VM counts
            for (node_id, zone, existing_vms) in [
                (1u64, "zone-a", 3), // 3 VMs
                (2u64, "zone-a", 2), // 2 VMs
                (3u64, "zone-b", 1), // 1 VM
                (4u64, "zone-c", 0), // 0 VMs
            ] {
                let capabilities = WorkerCapabilities {
                    cpu_cores: 8,
                    memory_mb: 16384,
                    disk_gb: 100,
                    features: vec!["microvm".to_string()],
                };
                let topology = NodeTopology {
                    datacenter: "us-east-1".to_string(),
                    zone: zone.to_string(),
                    rack: format!("rack-{}", node_id),
                };

                let worker_data =
                    bincode::serialize(&(format!("node{}", node_id), capabilities)).unwrap();
                worker_table
                    .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())
                    .unwrap();
                status_table
                    .insert(node_id.to_le_bytes().as_slice(), [0u8].as_slice())
                    .unwrap();
                topology_table
                    .insert(
                        node_id.to_le_bytes().as_slice(),
                        bincode::serialize(&topology).unwrap().as_slice(),
                    )
                    .unwrap();

                // Add existing VMs
                for i in 0..existing_vms {
                    let vm_state = blixard_core::types::VmState {
                        name: format!("existing-vm-{}-{}", node_id, i),
                        config: VmConfig {
                            name: format!("existing-vm-{}-{}", node_id, i),
                            config_path: "/test".to_string(),
                            vcpus: 1,
                            memory: 1024,
                            ..Default::default()
                        },
                        status: blixard_core::types::VmStatus::Running,
                        node_id,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };
                    vm_table
                        .insert(
                            &vm_state.name,
                            bincode::serialize(&vm_state).unwrap().as_slice(),
                        )
                        .unwrap();
                }
            }
        }
        write_txn.commit().unwrap();
    }

    // Test spreading across zones
    let vm_config = VmConfig {
        name: "spread-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        ..Default::default()
    };

    let strategy = PlacementStrategy::SpreadAcrossFailureDomains {
        min_domains: 3,
        spread_level: "zone".to_string(),
    };

    let decision = scheduler
        .schedule_vm_placement(&vm_config, strategy)
        .await
        .unwrap();
    // Should select node 4 (zone-c with 0 VMs)
    assert_eq!(decision.target_node_id, 4);

    // Test spreading at datacenter level
    let strategy2 = PlacementStrategy::SpreadAcrossFailureDomains {
        min_domains: 1,
        spread_level: "datacenter".to_string(),
    };

    let decision2 = scheduler
        .schedule_vm_placement(&vm_config, strategy2)
        .await
        .unwrap();
    // Should select any node (all in same datacenter)
    assert!(decision2.target_node_id >= 1 && decision2.target_node_id <= 4);
}
