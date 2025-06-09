# RSX Syntax and Patterns

RSX (React-like Syntax eXtension) is Dioxus's markup syntax for building UI components. This guide covers RSX syntax rules and patterns used in Seaglass.

## Basic RSX Rules

- All UI markup must be wrapped in the `rsx!` macro
- Elements use `{}` brackets instead of `<>` tags
- Attributes use `name: value` syntax (note the colon)
- Children are placed inside element brackets after attributes

## Component Structure

```rust
#[component]
pub fn MyComponent(prop1: String, prop2: Signal<bool>) -> Element {
    rsx! {
        div {
            class: "component-wrapper",
            // Attributes come first
            onclick: move |_| { /* handler */ },
            
            // Then children
            h1 { "Title" }
            p { "{prop1}" }
        }
    }
}
```

## Styling Patterns

```rust
// CSS classes (preferred for reusable styles)
class: "node-card transformation-card",

// Conditional classes
class: if selected() { "selected" } else { "" },
class: format!("menu-item {}", if disabled { "disabled" } else { "" }),

// Inline styles (for dynamic values)
style: "display: flex; gap: 8px; align-items: center;",
style: "width: {width}px; height: {height}px;",

// CSS variables for theming
style: "color: var(--color-text); background-color: var(--color-background);"
```

## Event Handlers

```rust
// Click handlers
onclick: move |_| {
    signal.set(!signal());
},

// Form inputs
oninput: move |evt: Event<FormData>| {
    signal.set(evt.value());
},

// Select dropdowns
onchange: move |evt: Event<FormData>| {
    handle_selection(evt.value());
},

// Mouse events for drag-and-drop
onmousedown: move |evt| {
    dragging.set(true);
    start_pos.set((evt.client_x(), evt.client_y()));
},
```

## Conditional Rendering

```rust
// Simple conditionals
if show_panel() {
    div { class: "panel", "Panel content" }
}

// Conditional attributes
div {
    class: if active { "active" },
    style: if visible { "display: block" } else { "display: none" },
}

// Match expressions for multiple cases
match transformation_type() {
    "Replace" => rsx! { ReplaceForm {} },
    "Filter" => rsx! { FilterForm {} },
    _ => rsx! { div { "Unknown transformation" } }
}
```

## Lists and Loops

```rust
// For loops with keys (ALWAYS use stable IDs, not indices)
for node in nodes.iter() {
    div {
        key: "{node.id}",  // IMPORTANT: Use stable IDs
        class: "list-item",
        "{node.name}"
    }
}

// Mapping over signals
{nodes().iter().map(|node| rsx! {
    NodeComponent {
        key: "{node.id}",
        node: node.clone(),
    }
})}
```

## Form Inputs

```rust
// Text input with validation
input {
    class: "input-field",
    r#type: "text",  // Note: r# prefix for reserved keywords
    value: "{field_value()}",
    oninput: move |evt| field_value.set(evt.value()),
    onblur: move |_| validate_field(),
    placeholder: "Enter value...",
}

// Select with options
select {
    class: "select-field",
    value: "{selected_option()}",
    onchange: move |evt| selected_option.set(evt.value()),
    
    option { value: "", disabled: true, "Select an option" }
    for opt in options() {
        option { value: "{opt.id}", "{opt.name}" }
    }
}
```

## Component Composition

```rust
// Parent component passing props
TransformationPanel {
    node_id: selected_node(),
    radio: global_radio.clone(),
    on_close: move || show_panel.set(false),
}

// Accepting children
#[component]
pub fn Container(children: Element) -> Element {
    rsx! {
        div { class: "container", {children} }
    }
}
```

## Performance Patterns

```rust
// Use memoization for expensive computations
let computed_data = use_memo(move || {
    expensive_calculation(raw_data())
});

// Use keys for list items
key: "{item.unique_id}",

// Avoid inline closures that capture too much
let handler = move |_| { /* minimal capture */ };
onclick: handler,
```

## Common Pitfalls to Avoid

1. **Missing keys in lists** - Always add unique, stable keys for list items
2. **Using index as key** - Never use array indices as keys
3. **Excessive inline styles** - Prefer CSS classes for reusable styles
4. **Forgetting `move` in closures** - Event handlers need `move` to capture variables
5. **Using `if-else` in RSX** - Use only `if` without else, or use match expressions
6. **Not escaping user input** - Be careful with `dangerous_inner_html`
7. **Incorrect attribute names** - Use snake_case, not camelCase

## Project-Specific Patterns

- Use `use_signal` for local component state
- Use `use_radio` for global state access
- Prefer CSS classes from Tailwind when possible
- Always specify types for event handlers (e.g., `Event<FormData>`)
- Use CSS variables for theme-aware styling