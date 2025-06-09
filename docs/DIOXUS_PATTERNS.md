# Dioxus Patterns and Best Practices

This guide covers Dioxus-specific patterns, hooks usage, and antipatterns to avoid in Seaglass.

## Hook Rules (MUST Follow)

1. **Always call hooks at the top level** of your component
2. **Never call hooks inside**:
   - Conditionals (`if` statements)
   - Loops (`for`, `while`)
   - Closures or nested functions
3. **Call hooks in the same order** every render
4. **Hooks must start with `use_`** prefix

## Core Dioxus Hooks

### 1. `use_signal` - Local State Management

```rust
#[component]
fn Counter() -> Element {
    let mut count = use_signal(|| 0);
    let name = use_signal(|| "World".to_string());
    
    rsx! {
        div {
            "Count: {count}"
            button { onclick: move |_| count += 1, "+" }
            input {
                value: "{name}",
                oninput: move |e| name.set(e.value())
            }
        }
    }
}
```

### 2. `use_memo` - Computed Values

```rust
#[component]
fn ExpensiveComponent(data: Signal<Vec<Item>>) -> Element {
    // Only recalculates when data changes
    let computed_total = use_memo(move || {
        data.read().iter().map(|item| item.price).sum::<f64>()
    });
    
    rsx! {
        div { "Total: ${computed_total}" }
    }
}
```

### 3. `use_effect` - Side Effects

```rust
#[component]
fn EffectExample() -> Element {
    let mut count = use_signal(|| 0);
    
    use_effect(move || {
        // This runs after every render where count changes
        println!("Count changed to: {}", count());
    });
    
    rsx! {
        button { onclick: move |_| count += 1, "Count: {count}" }
    }
}
```

### 4. `use_resource` - Async Data

```rust
#[component]
fn DataFetcher(user_id: Signal<u32>) -> Element {
    let user_data = use_resource(move || async move {
        fetch_user_data(user_id()).await
    });
    
    rsx! {
        match &*user_data.read() {
            Some(Ok(user)) => rsx! { UserProfile { user: user.clone() } },
            Some(Err(e)) => rsx! { div { "Error: {e}" } },
            None => rsx! { div { "Loading..." } }
        }
    }
}
```

### 5. `use_context` & `use_context_provider`

```rust
#[derive(Clone, Copy)]
struct ThemeContext {
    dark_mode: Signal<bool>,
}

#[component]
fn App() -> Element {
    // Provide context to all children
    use_context_provider(|| ThemeContext {
        dark_mode: Signal::new(false),
    });
    
    rsx! { ThemedButton {} }
}

#[component]
fn ThemedButton() -> Element {
    // Consume context in any child
    let theme = use_context::<ThemeContext>();
    
    rsx! {
        button {
            class: if theme.dark_mode() { "dark" } else { "light" },
            "Click me"
        }
    }
}
```

## Dioxus Antipatterns to Avoid

### 1. Using Index as Iterator Key
```rust
// ❌ BAD
for (idx, node) in nodes.iter().enumerate() {
    NodeCard { key: "{idx}", node: node }
}

// ✅ GOOD
for node in nodes.iter() {
    NodeCard { key: "{node.id}", node: node }
}
```

### 2. Cloning Large Data in Components
```rust
// ❌ BAD
let df_clone = dataframe.clone();

// ✅ GOOD
let df_rc = Rc::new(dataframe);
// Or use signals/radio for shared state
let df_signal = use_signal(|| dataframe);
```

### 3. Multiple State Reads in Render
```rust
// ❌ BAD
rsx! {
    div { "Nodes: {radio.read().nodes.len()}" }
    div { "Status: {radio.read().status}" }
}

// ✅ GOOD
let state = radio.read();
rsx! {
    div { "Nodes: {state.nodes.len()}" }
    div { "Status: {state.status}" }
}
```

### 4. State Updates During Render
```rust
// ❌ BAD - Causes infinite loop!
#[component]
fn MyComponent() -> Element {
    let mut count = use_signal(|| 0);
    
    if count() < 10 {
        count.set(count() + 1);  // Never do this!
    }
    
    rsx! { div { "{count}" } }
}

// ✅ GOOD
#[component]
fn MyComponent() -> Element {
    let mut count = use_signal(|| 0);
    
    use_effect(move || {
        if count() < 10 {
            count.set(count() + 1);
        }
    });
    
    rsx! { div { "{count}" } }
}
```

### 5. Complex Logic in Component Body
```rust
// ❌ BAD
#[component]
fn TransformationPanel() -> Element {
    // 200+ lines of parameter parsing logic
    let params = match transform_type {
        "Replace" => { /* 50 lines */ }
        "Filter" => { /* 50 lines */ }
    };
    
    rsx! { /* UI */ }
}

// ✅ GOOD
fn parse_transformation_params(transform_type: &str, input: &str) -> Params {
    // Logic extracted to separate function
}

#[component]
fn TransformationPanel() -> Element {
    let params = parse_transformation_params(transform_type, input);
    rsx! { /* UI */ }
}
```

### 6. Hooks in Conditionals or Loops
```rust
// ❌ BAD - Will panic!
if condition {
    let state = use_signal(|| 0);
}

for i in 0..count {
    let signal = use_signal(|| i);
}

// ✅ GOOD - All hooks at top level
let state = use_signal(|| 0);
let signals = (0..10).map(|_| use_signal(|| 0)).collect::<Vec<_>>();
```

## Best Practices

### Hook Initialization
```rust
#[component]
fn MyComponent() -> Element {
    // ALL hooks first
    let mut visible = use_signal(|| false);
    let data = use_memo(move || expensive_computation());
    let radio = use_radio::<AppState, Channel>();
    
    // Then other logic
    let computed_value = data() * 2;
    
    // Then RSX
    rsx! { /* ... */ }
}
```

### State Organization
```rust
// ❌ Too many signals
let mut field1 = use_signal(|| "");
let mut field2 = use_signal(|| "");
let mut field3 = use_signal(|| "");

// ✅ Group related state
#[derive(Default)]
struct FormData {
    field1: String,
    field2: String,
    field3: String,
}
let mut form = use_signal(|| FormData::default());
```

### Memoization
```rust
// ❌ Recalculates every render
let filtered = items.read().iter().filter(|i| i.active).collect::<Vec<_>>();

// ✅ Only recalculates when items change
let filtered = use_memo(move || {
    items.read().iter().filter(|i| i.active).collect::<Vec<_>>()
});
```

### Effect Cleanup
```rust
use_effect(move || {
    let handle = set_interval(1000, move || {
        count += 1;
    });
    
    // Cleanup function
    move || {
        clear_interval(handle);
    }
});
```

## Component Decomposition

```rust
// ❌ Monolithic component
fn App() -> Element {
    // All logic in one component
}

// ✅ Composed components
fn App() -> Element {
    rsx! {
        Header {}
        MainContent {}
        Footer {}
    }
}

// Extract hooks for reusable logic
fn use_user_data() -> Signal<Option<User>> {
    let mut user = use_signal(|| None);
    // Logic here
    user
}
```

## Performance Considerations

1. **Avoid unnecessary clones** - Use Rc/Arc for large data
2. **Minimize state reads** - Read once and destructure
3. **Use stable keys** - Critical for list performance
4. **Extract heavy computations** - Use use_memo
5. **Batch state updates** - Update multiple fields at once