# State Management in Seaglass

This guide covers state management patterns using dioxus-radio and dioxus-sdk storage features.

## dioxus-radio State Management

The project uses **dioxus-radio** (v0.3.0) for efficient state management across components. This library provides a pub-sub pattern that prevents unnecessary re-renders by allowing components to subscribe only to specific state changes.

### Key Concepts
- **Radio stations**: Global state containers initialized in the app root
- **Channels**: Enum-based subscriptions for granular state updates
- **Selective reactivity**: Only components subscribed to specific channels re-render

### Setting Up Radio State

```rust
// Define channels in src/models/data.rs
#[derive(Debug, Clone, PartialEq)]
pub enum DataChannel {
    PipelineUpdated,
    HistoryChanged,
    ThemeChanged,
    UiStateChanged,
}

// Initialize in main component
#[component]
fn App() -> Element {
    let pipeline = Pipeline::default();
    use_init_radio_station::<Pipeline, DataChannel>(pipeline);
    
    rsx! { /* app content */ }
}
```

### Using Radio in Components

```rust
// Import dioxus-radio
use dioxus_radio::prelude::*;

#[component]
fn MyComponent() -> Element {
    // Subscribe to state
    let mut radio = use_radio::<Pipeline, DataChannel>();
    let pipeline = radio.read();
    
    rsx! {
        div { "Nodes: {pipeline.nodes.len()}" }
        button {
            onclick: move |_| {
                // Update state and broadcast
                radio.write().add_node(new_node);
                radio.write().broadcast(DataChannel::PipelineUpdated);
            },
            "Add Node"
        }
    }
}
```

### Best Practices

1. **Define clear channel boundaries** - Each channel should represent a specific state change
2. **Keep channel granularity appropriate** - Not too broad, not too narrow
3. **Initialize radio stations at the app root level**
4. **Use selective subscriptions** to optimize performance
5. **Broadcast changes immediately** after state mutations

### When to Use dioxus-radio
- Managing global application state that multiple components need
- Preventing cascading re-renders in complex UI hierarchies
- Synchronizing state across distant components
- Implementing undo/redo or history features

## dioxus-sdk Storage

The project uses **dioxus-sdk** (v0.6.0) storage feature for persisting application state.

### LocalStorage API

```rust
use dioxus_sdk::storage::LocalStorage;

// Save state
LocalStorage::set("pipeline_state", &pipeline).ok();
LocalStorage::set("theme", &theme_name).ok();

// Load state with fallback
fn get_or_init<T, F>(key: &str, init_fn: F) -> T 
where 
    T: for<'de> Deserialize<'de> + Serialize,
    F: FnOnce() -> T,
{
    LocalStorage::get(key).unwrap_or_else(|_| init_fn())
}

// Usage
let pipeline = get_or_init("pipeline_state", Pipeline::default);
```

### Storage Best Practices

1. **Error Handling**
```rust
// Always handle storage errors gracefully
if let Err(e) = LocalStorage::set("key", &value) {
    log::error!("Failed to save to localStorage: {}", e);
}
```

2. **Size Limitations**
- localStorage has a ~5-10MB limit in browsers
- Large DataFrames should use the custom cache system instead
- Consider compressing data before storage

3. **Type Safety**
```rust
// Define storage keys as constants
const PIPELINE_KEY: &str = "pipeline_state";
const THEME_KEY: &str = "user_theme";
const HISTORY_KEY: &str = "pipeline_history";
```

4. **Migration Strategy**
```rust
// Version your storage format
#[derive(Serialize, Deserialize)]
struct StoredState {
    version: u32,
    pipeline: Pipeline,
}

// Handle version migrations
let stored = LocalStorage::get::<StoredState>(KEY)?;
let pipeline = match stored.version {
    1 => migrate_v1_to_v2(stored.pipeline),
    2 => stored.pipeline,
    _ => Pipeline::default(),
};
```

## State Management Patterns

### Single Source of Truth
```rust
// ❌ BAD: Synchronizing between different state systems
use_effect(move || {
    let radio_value = radio.read().value;
    signal.set(radio_value);  // Manual sync
});

// ✅ GOOD: Use a single source of truth
let value = radio.read().value;  // Read directly from radio
```

### Avoid State Duplication
```rust
// ❌ BAD: Duplicating state
let mut local_nodes = use_signal(|| radio.read().nodes.clone());

// ✅ GOOD: Reference global state
let nodes = radio.read().nodes;
```

### Performance Optimization
```rust
// ❌ BAD: Multiple reads
rsx! {
    div { "Count: {radio.read().count}" }
    div { "Name: {radio.read().name}" }
}

// ✅ GOOD: Single read
let state = radio.read();
rsx! {
    div { "Count: {state.count}" }
    div { "Name: {state.name}" }
}
```

## Platform Considerations

- **Web**: LocalStorage works seamlessly
- **Desktop**: Data stored in platform-specific locations
- **Mobile**: Not yet supported by Seaglass

## Future SDK Integration Opportunities

The dioxus-sdk offers additional features that could enhance Seaglass:

1. **Clipboard**: Copy transformation results to clipboard
2. **Notifications**: Alert users when long-running pipelines complete
3. **File System Access**: Direct file operations (when SDK adds this feature)
4. **Network Status**: Check connectivity before cloud operations

## Important Notes

- The SDK is under active development - expect breaking changes
- Always specify exact versions in Cargo.toml
- Not all features work on all platforms
- Prefer dioxus-radio for runtime state, LocalStorage for persistence