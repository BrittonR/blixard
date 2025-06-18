use blixard::raft_codec::*;
use raft::prelude::*;
use proptest::prelude::*;

mod unit_tests {
    use super::*;
    
    #[test]
    fn test_entry_roundtrip_basic() {
        let entry = Entry {
            entry_type: EntryType::EntryNormal as i32,
            term: 42,
            index: 100,
            data: vec![1, 2, 3, 4, 5],
            context: vec![10, 20, 30],
            sync_log: true,
        };
        
        let serialized = serialize_entry(&entry).unwrap();
        let deserialized = deserialize_entry(&serialized).unwrap();
        
        assert_eq!(entry.entry_type, deserialized.entry_type);
        assert_eq!(entry.term, deserialized.term);
        assert_eq!(entry.index, deserialized.index);
        assert_eq!(entry.data, deserialized.data);
        assert_eq!(entry.context, deserialized.context);
        assert_eq!(entry.sync_log, deserialized.sync_log);
    }
    
    #[test]
    fn test_entry_empty_data() {
        let entry = Entry {
            entry_type: EntryType::EntryConfChange as i32,
            term: 0,
            index: 0,
            data: vec![],
            context: vec![],
            sync_log: false,
        };
        
        let serialized = serialize_entry(&entry).unwrap();
        let deserialized = deserialize_entry(&serialized).unwrap();
        
        assert_eq!(entry.data, deserialized.data);
        assert_eq!(entry.context, deserialized.context);
    }
    
    #[test]
    fn test_entry_large_data() {
        let entry = Entry {
            entry_type: EntryType::EntryNormal as i32,
            term: u64::MAX,
            index: u64::MAX,
            data: vec![0xFF; 10000],
            context: vec![0xAB; 5000],
            sync_log: true,
        };
        
        let serialized = serialize_entry(&entry).unwrap();
        let deserialized = deserialize_entry(&serialized).unwrap();
        
        assert_eq!(entry.data.len(), deserialized.data.len());
        assert_eq!(entry.context.len(), deserialized.context.len());
        assert_eq!(entry.data, deserialized.data);
        assert_eq!(entry.context, deserialized.context);
    }
    
    #[test]
    fn test_entry_deserialize_insufficient_data() {
        let data = vec![0u8; 10]; // Too small
        let result = deserialize_entry(&data);
        assert!(result.is_err());
        
        if let Err(e) = result {
            match e {
                blixard::error::BlixardError::Serialization { operation, .. } => {
                    assert_eq!(operation, "deserialize entry");
                }
                _ => panic!("Expected serialization error"),
            }
        }
    }
    
    #[test]
    fn test_hard_state_roundtrip() {
        let hs = HardState {
            term: 42,
            vote: 3,
            commit: 100,
        };
        
        let serialized = serialize_hard_state(&hs).unwrap();
        assert_eq!(serialized.len(), 24); // 3 * 8 bytes
        
        let deserialized = deserialize_hard_state(&serialized).unwrap();
        
        assert_eq!(hs.term, deserialized.term);
        assert_eq!(hs.vote, deserialized.vote);
        assert_eq!(hs.commit, deserialized.commit);
    }
    
    #[test]
    fn test_hard_state_boundary_values() {
        let hs = HardState {
            term: u64::MAX,
            vote: 0,
            commit: u64::MAX / 2,
        };
        
        let serialized = serialize_hard_state(&hs).unwrap();
        let deserialized = deserialize_hard_state(&serialized).unwrap();
        
        assert_eq!(hs.term, deserialized.term);
        assert_eq!(hs.vote, deserialized.vote);
        assert_eq!(hs.commit, deserialized.commit);
    }
    
    #[test]
    fn test_hard_state_deserialize_wrong_size() {
        // Too small
        let data = vec![0u8; 10];
        assert!(deserialize_hard_state(&data).is_err());
        
        // Too large
        let data = vec![0u8; 30];
        assert!(deserialize_hard_state(&data).is_err());
        
        // Just right
        let data = vec![0u8; 24];
        assert!(deserialize_hard_state(&data).is_ok());
    }
    
    #[test]
    fn test_conf_state_empty() {
        let cs = ConfState {
            voters: vec![],
            learners: vec![],
            voters_outgoing: vec![],
            learners_next: vec![],
            auto_leave: false,
        };
        
        let serialized = serialize_conf_state(&cs).unwrap();
        let deserialized = deserialize_conf_state(&serialized).unwrap();
        
        assert!(deserialized.voters.is_empty());
        assert!(deserialized.learners.is_empty());
        assert!(deserialized.voters_outgoing.is_empty());
        assert!(deserialized.learners_next.is_empty());
        assert_eq!(cs.auto_leave, deserialized.auto_leave);
    }
    
    #[test]
    fn test_conf_state_populated() {
        let cs = ConfState {
            voters: vec![1, 2, 3],
            learners: vec![4, 5],
            voters_outgoing: vec![6],
            learners_next: vec![7, 8, 9, 10],
            auto_leave: true,
        };
        
        let serialized = serialize_conf_state(&cs).unwrap();
        let deserialized = deserialize_conf_state(&serialized).unwrap();
        
        assert_eq!(cs.voters, deserialized.voters);
        assert_eq!(cs.learners, deserialized.learners);
        assert_eq!(cs.voters_outgoing, deserialized.voters_outgoing);
        assert_eq!(cs.learners_next, deserialized.learners_next);
        assert_eq!(cs.auto_leave, deserialized.auto_leave);
    }
    
    #[test]
    fn test_conf_state_large_membership() {
        let cs = ConfState {
            voters: (1..=100).collect(),
            learners: (101..=150).collect(),
            voters_outgoing: vec![],
            learners_next: (151..=200).collect(),
            auto_leave: false,
        };
        
        let serialized = serialize_conf_state(&cs).unwrap();
        let deserialized = deserialize_conf_state(&serialized).unwrap();
        
        assert_eq!(cs.voters.len(), 100);
        assert_eq!(cs.learners.len(), 50);
        assert_eq!(cs.learners_next.len(), 50);
        
        assert_eq!(cs.voters, deserialized.voters);
        assert_eq!(cs.learners, deserialized.learners);
        assert_eq!(cs.learners_next, deserialized.learners_next);
    }
    
    #[test]
    fn test_message_roundtrip_basic() {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgHup);
        msg.to = 2;
        msg.from = 1;
        msg.term = 42;
        msg.index = 100;
        msg.commit = 50;
        
        let serialized = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message(&serialized).unwrap();
        
        assert_eq!(msg.get_msg_type(), deserialized.get_msg_type());
        assert_eq!(msg.to, deserialized.to);
        assert_eq!(msg.from, deserialized.from);
        assert_eq!(msg.term, deserialized.term);
        assert_eq!(msg.index, deserialized.index);
        assert_eq!(msg.commit, deserialized.commit);
    }
    
    #[test]
    fn test_message_with_entries() {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        msg.to = 2;
        msg.from = 1;
        msg.term = 42;
        
        // Add some entries
        for i in 0..3 {
            let mut entry = Entry::default();
            entry.index = i;
            entry.term = 42;
            entry.data = vec![i as u8; 10];
            msg.entries.push(entry);
        }
        
        let serialized = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message(&serialized).unwrap();
        
        assert_eq!(msg.entries.len(), deserialized.entries.len());
        for (orig, deser) in msg.entries.iter().zip(deserialized.entries.iter()) {
            assert_eq!(orig.index, deser.index);
            assert_eq!(orig.term, deser.term);
            assert_eq!(orig.data, deser.data);
        }
    }
    
    #[test]
    fn test_message_deserialize_invalid() {
        // Invalid protobuf data
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = deserialize_message(&data);
        
        // Should either fail or produce a message with default values
        // Protobuf is lenient about unknown fields
        match result {
            Ok(msg) => {
                // If it succeeds, it should have reasonable defaults
                assert_eq!(msg.get_msg_type(), MessageType::MsgHup); // Default
            }
            Err(e) => {
                match e {
                    blixard::error::BlixardError::Serialization { operation, .. } => {
                        assert_eq!(operation, "deserialize raft message");
                    }
                    _ => panic!("Expected serialization error"),
                }
            }
        }
    }
}

// Property-based tests
proptest! {
    #[test]
    fn prop_entry_roundtrip(
        entry_type in 0i32..5,
        term in any::<u64>(),
        index in any::<u64>(),
        data in prop::collection::vec(any::<u8>(), 0..1000),
        context in prop::collection::vec(any::<u8>(), 0..100),
        sync_log in any::<bool>()
    ) {
        let entry = Entry {
            entry_type,
            term,
            index,
            data,
            context,
            sync_log,
        };
        
        let serialized = serialize_entry(&entry).unwrap();
        let deserialized = deserialize_entry(&serialized).unwrap();
        
        prop_assert_eq!(entry.entry_type, deserialized.entry_type);
        prop_assert_eq!(entry.term, deserialized.term);
        prop_assert_eq!(entry.index, deserialized.index);
        prop_assert_eq!(entry.data, deserialized.data);
        prop_assert_eq!(entry.context, deserialized.context);
        prop_assert_eq!(entry.sync_log, deserialized.sync_log);
    }
    
    #[test]
    fn prop_hard_state_roundtrip(
        term in any::<u64>(),
        vote in any::<u64>(),
        commit in any::<u64>()
    ) {
        let hs = HardState { term, vote, commit };
        
        let serialized = serialize_hard_state(&hs).unwrap();
        let deserialized = deserialize_hard_state(&serialized).unwrap();
        
        prop_assert_eq!(hs.term, deserialized.term);
        prop_assert_eq!(hs.vote, deserialized.vote);
        prop_assert_eq!(hs.commit, deserialized.commit);
    }
    
    #[test]
    fn prop_conf_state_roundtrip(
        voters in prop::collection::vec(any::<u64>(), 0..20),
        learners in prop::collection::vec(any::<u64>(), 0..10),
        voters_outgoing in prop::collection::vec(any::<u64>(), 0..10),
        learners_next in prop::collection::vec(any::<u64>(), 0..10),
        auto_leave in any::<bool>()
    ) {
        let cs = ConfState {
            voters,
            learners,
            voters_outgoing,
            learners_next,
            auto_leave,
        };
        
        let serialized = serialize_conf_state(&cs).unwrap();
        let deserialized = deserialize_conf_state(&serialized).unwrap();
        
        prop_assert_eq!(cs.voters, deserialized.voters);
        prop_assert_eq!(cs.learners, deserialized.learners);
        prop_assert_eq!(cs.voters_outgoing, deserialized.voters_outgoing);
        prop_assert_eq!(cs.learners_next, deserialized.learners_next);
        prop_assert_eq!(cs.auto_leave, deserialized.auto_leave);
    }
    
    #[test]
    fn prop_entry_serialization_size(
        data_len in 0usize..10000,
        context_len in 0usize..1000
    ) {
        let entry = Entry {
            entry_type: 0,
            term: 42,
            index: 100,
            data: vec![0u8; data_len],
            context: vec![0u8; context_len],
            sync_log: true,
        };
        
        let serialized = serialize_entry(&entry).unwrap();
        
        // Expected size: 8 (index) + 8 (term) + 1 (type) + 4 (data_len) + data_len + 
        //                4 (context_len) + context_len + 1 (sync_log)
        let expected_size = 8 + 8 + 1 + 4 + data_len + 4 + context_len + 1;
        prop_assert_eq!(serialized.len(), expected_size);
    }
    
    #[test]
    fn prop_corrupted_entry_data(
        _valid_data in prop::collection::vec(any::<u8>(), 30..1000),
        corruption_index in 0usize..30,
        corruption_value in any::<u8>()
    ) {
        // Create a valid entry first
        let entry = Entry {
            entry_type: 0,
            term: 42,
            index: 100,
            data: vec![1, 2, 3],
            context: vec![4, 5, 6],
            sync_log: true,
        };
        
        let mut serialized = serialize_entry(&entry).unwrap();
        
        // Corrupt the data at the specified index if within bounds
        if corruption_index < serialized.len() {
            serialized[corruption_index] = corruption_value;
            
            // Try to deserialize - it might succeed with wrong values or fail
            let result = deserialize_entry(&serialized);
            
            // If it succeeds, we can't make strong assertions about the values
            // since corruption might just change the data content
            if result.is_ok() {
                // At least check it doesn't panic
                let _ = result.unwrap();
            }
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    
    #[test]
    fn test_large_message_performance() {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        
        // Add 1000 entries
        for i in 0..1000 {
            let mut entry = Entry::default();
            entry.index = i;
            entry.term = 42;
            entry.data = vec![i as u8; 100]; // 100 bytes each
            msg.entries.push(entry);
        }
        
        let start = std::time::Instant::now();
        let serialized = serialize_message(&msg).unwrap();
        let serialize_time = start.elapsed();
        
        let start = std::time::Instant::now();
        let _deserialized = deserialize_message(&serialized).unwrap();
        let deserialize_time = start.elapsed();
        
        // Just ensure it completes in reasonable time
        assert!(serialize_time.as_millis() < 100, "Serialization took too long: {:?}", serialize_time);
        assert!(deserialize_time.as_millis() < 100, "Deserialization took too long: {:?}", deserialize_time);
    }
}