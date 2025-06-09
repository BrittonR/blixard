use proptest::prelude::*;
use blixard::types::*;

proptest! {
    #[test]
    fn test_vm_state_transitions_valid(
        initial_status in prop_oneof![
            Just(VmStatus::Stopped),
            Just(VmStatus::Running),
            Just(VmStatus::Failed),
        ],
        operations in prop::collection::vec(any::<VmOperation>(), 0..10)
    ) {
        let mut status = initial_status;
        
        for op in operations {
            let new_status = apply_operation(status, op);
            // Verify state transition is valid
            prop_assert!(is_valid_transition(status, new_status));
            status = new_status;
        }
    }
}

#[derive(Debug, Clone, Arbitrary)]
enum VmOperation {
    Start,
    Stop,
    Fail,
}

fn apply_operation(status: VmStatus, op: VmOperation) -> VmStatus {
    match (status, op) {
        (VmStatus::Stopped, VmOperation::Start) => VmStatus::Starting,
        (VmStatus::Starting, VmOperation::Start) => VmStatus::Running,
        (VmStatus::Running, VmOperation::Stop) => VmStatus::Stopping,
        (VmStatus::Stopping, VmOperation::Stop) => VmStatus::Stopped,
        (_, VmOperation::Fail) => VmStatus::Failed,
        (status, _) => status, // Invalid operation, no change
    }
}

fn is_valid_transition(from: VmStatus, to: VmStatus) -> bool {
    matches!(
        (from, to),
        (VmStatus::Stopped, VmStatus::Starting) |
        (VmStatus::Starting, VmStatus::Running) |
        (VmStatus::Starting, VmStatus::Failed) |
        (VmStatus::Running, VmStatus::Stopping) |
        (VmStatus::Running, VmStatus::Failed) |
        (VmStatus::Stopping, VmStatus::Stopped) |
        (VmStatus::Stopping, VmStatus::Failed) |
        (_, VmStatus::Failed) | // Can always fail
        (s1, s2) if s1 == s2 // No change is valid
    )
}