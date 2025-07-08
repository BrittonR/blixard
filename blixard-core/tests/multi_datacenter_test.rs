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
async fn test_datacenter_topology_storage() {
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

    // Create topology information
    let topology1 = NodeTopology {
        datacenter: "us-east-1".to_string(),
        zone: "zone-a".to_string(),
        rack: "rack-1".to_string(),
    };

    let topology2 = NodeTopology {
        datacenter: "us-west-1".to_string(),
        zone: "zone-b".to_string(),
        rack: "rack-2".to_string(),
    };

    // Store topologies
    {
        let write_txn = db.begin_write().unwrap();
        {
            let mut topology_table = write_txn.open_table(NODE_TOPOLOGY_TABLE).unwrap();

            let data1 = bincode::serialize(&topology1).unwrap();
            topology_table
                .insert(1u64.to_le_bytes().as_slice(), data1.as_slice())
                .unwrap();

            let data2 = bincode::serialize(&topology2).unwrap();
            topology_table
                .insert(2u64.to_le_bytes().as_slice(), data2.as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();
    }

    // Read back topologies
    {
        let read_txn = db.begin_read().unwrap();
        let topology_table = read_txn.open_table(NODE_TOPOLOGY_TABLE).unwrap();

        let data1 = topology_table
            .get(1u64.to_le_bytes().as_slice())
            .unwrap()
            .unwrap();
        let loaded_topology1: NodeTopology = bincode::deserialize(data1.value()).unwrap();
        assert_eq!(loaded_topology1.datacenter, "us-east-1");
        assert_eq!(loaded_topology1.zone, "zone-a");
        assert_eq!(loaded_topology1.rack, "rack-1");

        let data2 = topology_table
            .get(2u64.to_le_bytes().as_slice())
            .unwrap()
            .unwrap();
        let loaded_topology2: NodeTopology = bincode::deserialize(data2.value()).unwrap();
        assert_eq!(loaded_topology2.datacenter, "us-west-1");
        assert_eq!(loaded_topology2.zone, "zone-b");
        assert_eq!(loaded_topology2.rack, "rack-2");
    }
}

#[tokio::test]
async fn test_locality_aware_placement() {
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

            // Node 1: us-east-1
            let node1_id = 1u64;
            let capabilities1 = WorkerCapabilities {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_gb: 100,
                features: vec!["microvm".to_string()],
            };
            let topology1 = NodeTopology {
                datacenter: "us-east-1".to_string(),
                zone: "zone-a".to_string(),
                rack: "rack-1".to_string(),
            };

            let worker_data1 = bincode::serialize(&("node1".to_string(), capabilities1)).unwrap();
            worker_table
                .insert(node1_id.to_le_bytes().as_slice(), worker_data1.as_slice())
                .unwrap();
            status_table
                .insert(node1_id.to_le_bytes().as_slice(), [0u8].as_slice())
                .unwrap();
            topology_table
                .insert(
                    node1_id.to_le_bytes().as_slice(),
                    bincode::serialize(&topology1).unwrap().as_slice(),
                )
                .unwrap();

            // Node 2: us-west-1
            let node2_id = 2u64;
            let capabilities2 = WorkerCapabilities {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_gb: 100,
                features: vec!["microvm".to_string()],
            };
            let topology2 = NodeTopology {
                datacenter: "us-west-1".to_string(),
                zone: "zone-b".to_string(),
                rack: "rack-2".to_string(),
            };

            let worker_data2 = bincode::serialize(&("node2".to_string(), capabilities2)).unwrap();
            worker_table
                .insert(node2_id.to_le_bytes().as_slice(), worker_data2.as_slice())
                .unwrap();
            status_table
                .insert(node2_id.to_le_bytes().as_slice(), [0u8].as_slice())
                .unwrap();
            topology_table
                .insert(
                    node2_id.to_le_bytes().as_slice(),
                    bincode::serialize(&topology2).unwrap().as_slice(),
                )
                .unwrap();

            // Node 3: us-east-1 (different zone)
            let node3_id = 3u64;
            let capabilities3 = WorkerCapabilities {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_gb: 100,
                features: vec!["microvm".to_string()],
            };
            let topology3 = NodeTopology {
                datacenter: "us-east-1".to_string(),
                zone: "zone-b".to_string(),
                rack: "rack-3".to_string(),
            };

            let worker_data3 = bincode::serialize(&("node3".to_string(), capabilities3)).unwrap();
            worker_table
                .insert(node3_id.to_le_bytes().as_slice(), worker_data3.as_slice())
                .unwrap();
            status_table
                .insert(node3_id.to_le_bytes().as_slice(), [0u8].as_slice())
                .unwrap();
            topology_table
                .insert(
                    node3_id.to_le_bytes().as_slice(),
                    bincode::serialize(&topology3).unwrap().as_slice(),
                )
                .unwrap();
        }
        write_txn.commit().unwrap();
    }

    // Test 1: Prefer specific datacenter
    let vm_config1 = VmConfig {
        name: "prefer-east-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        locality_preference: LocalityPreference {
            preferred_datacenter: Some("us-east-1".to_string()),
            preferred_zone: None,
            enforce_same_datacenter: false,
            spread_across_zones: false,
            max_latency_ms: None,
            excluded_datacenters: vec![],
        },
        ..Default::default()
    };

    let decision1 = scheduler
        .schedule_vm_placement(&vm_config1, PlacementStrategy::MostAvailable)
        .await
        .unwrap();
    // Should select either node 1 or 3 (both in us-east-1)
    assert!(decision1.selected_node_id == 1 || decision1.selected_node_id == 3);

    // Test 2: Exclude specific datacenter
    let vm_config2 = VmConfig {
        name: "exclude-west-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        locality_preference: LocalityPreference {
            preferred_datacenter: None,
            preferred_zone: None,
            enforce_same_datacenter: false,
            spread_across_zones: false,
            max_latency_ms: None,
            excluded_datacenters: vec!["us-west-1".to_string()],
        },
        ..Default::default()
    };

    let decision2 = scheduler
        .schedule_vm_placement(&vm_config2, PlacementStrategy::MostAvailable)
        .await
        .unwrap();
    // Should not select node 2 (in us-west-1)
    assert!(decision2.selected_node_id == 1 || decision2.selected_node_id == 3);

    // Test 3: Prefer specific zone within datacenter
    let vm_config3 = VmConfig {
        name: "prefer-zone-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        locality_preference: LocalityPreference {
            preferred_datacenter: Some("us-east-1".to_string()),
            preferred_zone: Some("zone-b".to_string()),
            enforce_same_datacenter: false,
            spread_across_zones: false,
            max_latency_ms: None,
            excluded_datacenters: vec![],
        },
        ..Default::default()
    };

    let decision3 = scheduler
        .schedule_vm_placement(&vm_config3, PlacementStrategy::MostAvailable)
        .await
        .unwrap();
    // Should select node 3 (us-east-1, zone-b)
    assert_eq!(decision3.selected_node_id, 3);
}

#[tokio::test]
async fn test_datacenter_latency_tracking() {
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

    let scheduler = VmScheduler::new(db);

    // Update latency between datacenters
    scheduler
        .update_datacenter_latency("us-east-1", "us-west-1", 50)
        .await;
    scheduler
        .update_datacenter_latency("us-east-1", "eu-west-1", 100)
        .await;
    scheduler
        .update_datacenter_latency("us-west-1", "eu-west-1", 150)
        .await;

    // Verify latencies are stored bidirectionally
    assert_eq!(
        scheduler
            .get_datacenter_latency("us-east-1", "us-west-1")
            .await,
        Some(50)
    );
    assert_eq!(
        scheduler
            .get_datacenter_latency("us-west-1", "us-east-1")
            .await,
        Some(50)
    );
    assert_eq!(
        scheduler
            .get_datacenter_latency("us-east-1", "eu-west-1")
            .await,
        Some(100)
    );
    assert_eq!(
        scheduler
            .get_datacenter_latency("eu-west-1", "us-east-1")
            .await,
        Some(100)
    );
    assert_eq!(
        scheduler
            .get_datacenter_latency("us-west-1", "eu-west-1")
            .await,
        Some(150)
    );
    assert_eq!(
        scheduler
            .get_datacenter_latency("eu-west-1", "us-west-1")
            .await,
        Some(150)
    );

    // Non-existent route
    assert_eq!(
        scheduler
            .get_datacenter_latency("us-east-1", "ap-southeast-1")
            .await,
        None
    );
}
