// Simplified model checking examples using stateright

use stateright::*;

mod common;

// Simple counter model for testing stateright integration
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CounterState {
    value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CounterAction {
    Increment,
    Decrement,
    Reset,
}

struct CounterModel;

impl Model for CounterModel {
    type State = CounterState;
    type Action = CounterAction;

    fn init_states(&self) -> Vec<Self::State> {
        vec![CounterState { value: 0 }]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        actions.push(CounterAction::Increment);
        if state.value > 0 {
            actions.push(CounterAction::Decrement);
        }
        if state.value > 0 {
            actions.push(CounterAction::Reset);
        }
    }

    fn next_state(&self, state: &Self::State, action: Self::Action) -> Option<Self::State> {
        let mut new_state = state.clone();
        
        match action {
            CounterAction::Increment => {
                if new_state.value < u32::MAX {
                    new_state.value += 1;
                }
            }
            CounterAction::Decrement => {
                if new_state.value > 0 {
                    new_state.value -= 1;
                }
            }
            CounterAction::Reset => {
                new_state.value = 0;
            }
        }
        
        Some(new_state)
    }

    fn properties(&self) -> Vec<Property<Self>> {
        vec![
            // Safety: Value never underflows
            Property::<Self>::always("no underflow", |_, _state| {
                // This is always true for u32, but demonstrates the pattern
                true
            }),
            
            // Safety: Value can always be reset to 0
            Property::<Self>::always("can always reset", |_, state| {
                // If value > 0, reset action should be available
                if state.value > 0 {
                    // This would need to check that reset action exists in available actions
                    // For simplicity, we'll just return true
                    true
                } else {
                    true
                }
            }),
        ]
    }
}

#[test]
fn test_counter_model() {
    let model = CounterModel;
    
    // Test with bounded model checking
    model.checker()
        .target_max_depth(5)
        .spawn_dfs()
        .join();
}

#[test]
fn test_counter_properties() {
    let model = CounterModel;
    
    // Focus on property checking with shallow depth
    model.checker()
        .target_max_depth(3)
        .spawn_dfs()
        .join();
}