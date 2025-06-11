use blixard::types::*;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum VmOperation {
    Start,
    Stop,
    Fail,
}

impl Arbitrary for VmOperation {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(VmOperation::Start),
            Just(VmOperation::Stop),
            Just(VmOperation::Fail),
        ]
        .boxed()
    }
}

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
    use VmStatus::*;
    match (from, to) {
        (Stopped, Starting) => true,
        (Starting, Running) => true,
        (Starting, Failed) => true,
        (Running, Stopping) => true,
        (Running, Failed) => true,
        (Stopping, Stopped) => true,
        (Stopping, Failed) => true,
        (_, Failed) => true,          // Can always fail
        (s1, s2) if s1 == s2 => true, // No change is valid
        _ => false,
    }
}
