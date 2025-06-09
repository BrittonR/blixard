# Rust Code Guidelines

This document outlines Rust-specific coding principles and patterns used in Seaglass.

## Core Principles

### 1. Modular Design
- **Single Responsibility**: Each function should do ONE thing well
- **Small Functions**: Aim for functions under 20 lines
- **Clear Naming**: Function names should describe what they do, not how
- **Minimal Dependencies**: Functions should depend on the minimum necessary inputs

### 2. Composability
- **Pure Functions First**: Prefer pure functions that don't modify state
- **Return Types Over Side Effects**: Return results instead of modifying parameters
- **Use Traits**: Define behavior through traits for maximum flexibility
- **Builder Pattern**: Use builder pattern for complex object construction

### 3. Testability
- **Dependency Injection**: Accept dependencies as parameters
- **Mock-friendly Interfaces**: Use traits for external dependencies
- **Isolated Logic**: Separate business logic from I/O operations
- **No Hidden State**: Avoid global state and implicit dependencies

## Rust-Specific Patterns

### Function Decomposition
```rust
// ❌ BAD: Monolithic function
fn process_user_data(data: &str) -> Result<User, Error> {
    // validation logic mixed with parsing
    // business rules mixed with data transformation
    // error handling scattered throughout
}

// ✅ GOOD: Decomposed functions
fn validate_input(data: &str) -> Result<&str, ValidationError> { }
fn parse_user_data(data: &str) -> Result<RawUser, ParseError> { }
fn apply_business_rules(raw: RawUser) -> Result<User, BusinessError> { }
fn process_user_data(data: &str) -> Result<User, Error> {
    validate_input(data)?
        .pipe(parse_user_data)?
        .pipe(apply_business_rules)
}
```

### Trait-Based Composition
```rust
// Define behavior through traits
trait Validator<T> {
    fn validate(&self, item: &T) -> Result<(), ValidationError>;
}

trait Parser<T, U> {
    fn parse(&self, input: T) -> Result<U, ParseError>;
}

// Compose behaviors
struct Pipeline<V: Validator<String>, P: Parser<String, User>> {
    validator: V,
    parser: P,
}
```

### Error Handling
```rust
// Use custom error types for each module
#[derive(Debug, thiserror::Error)]
enum ModuleError {
    #[error("Validation failed: {0}")]
    Validation(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

// Compose errors at module boundaries
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error(transparent)]
    Module(#[from] ModuleError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

## Testing Strategy

### Unit Test Structure
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test each function in isolation
    #[test]
    fn test_validate_input() {
        // Given
        let input = "test data";
        
        // When
        let result = validate_input(input);
        
        // Then
        assert!(result.is_ok());
    }

    // Use property-based testing for complex logic
    #[quickcheck]
    fn prop_parse_never_panics(input: String) -> bool {
        parse_user_data(&input).is_ok() || parse_user_data(&input).is_err()
    }
}
```

### Mock Dependencies
```rust
// Define trait for external dependencies
trait Database {
    fn get_user(&self, id: u64) -> Result<User, DbError>;
}

// Create mock for testing
#[cfg(test)]
struct MockDatabase {
    users: HashMap<u64, User>,
}

#[cfg(test)]
impl Database for MockDatabase {
    fn get_user(&self, id: u64) -> Result<User, DbError> {
        self.users.get(&id).cloned().ok_or(DbError::NotFound)
    }
}
```

## Code Review Checklist

When reviewing or writing code, ensure:

- [ ] Functions are under 20 lines
- [ ] Each function has a single, clear purpose
- [ ] Dependencies are injected, not hardcoded
- [ ] Business logic is separated from I/O
- [ ] Error types are specific and informative
- [ ] Tests exist for each public function
- [ ] Complex functions are decomposed into smaller ones
- [ ] Traits are used for polymorphic behavior
- [ ] State mutations are minimized
- [ ] Functions can be tested in isolation

## Refactoring Example

### Before (Monolithic)
```rust
fn process_order(order_id: u64) -> Result<(), Error> {
    let conn = get_db_connection()?;
    let order = conn.query("SELECT * FROM orders WHERE id = ?", &[order_id])?;
    if order.status != "pending" {
        return Err(Error::InvalidStatus);
    }
    let items = conn.query("SELECT * FROM order_items WHERE order_id = ?", &[order_id])?;
    let total = items.iter().map(|i| i.price * i.quantity).sum();
    if total > 1000.0 {
        let discount = total * 0.1;
        let final_total = total - discount;
        conn.execute("UPDATE orders SET total = ?, discount = ? WHERE id = ?", 
                     &[final_total, discount, order_id])?;
    } else {
        conn.execute("UPDATE orders SET total = ? WHERE id = ?", &[total, order_id])?;
    }
    conn.execute("UPDATE orders SET status = 'processed' WHERE id = ?", &[order_id])?;
    send_email(&order.customer_email, "Order processed")?;
    Ok(())
}
```

### After (Decomposed)
```rust
// Domain types
struct Order { id: u64, status: Status, customer_email: String }
struct OrderItem { price: f64, quantity: u32 }
struct PricingResult { total: f64, discount: f64 }

// Pure business logic
fn calculate_pricing(items: &[OrderItem]) -> PricingResult {
    let total: f64 = items.iter().map(|i| i.price * f64::from(i.quantity)).sum();
    let discount = if total > 1000.0 { total * 0.1 } else { 0.0 };
    PricingResult { total: total - discount, discount }
}

fn validate_order_status(order: &Order) -> Result<(), DomainError> {
    match order.status {
        Status::Pending => Ok(()),
        _ => Err(DomainError::InvalidStatus(order.status)),
    }
}

// Repository trait
trait OrderRepository {
    fn get_order(&self, id: u64) -> Result<Order, RepoError>;
    fn get_order_items(&self, order_id: u64) -> Result<Vec<OrderItem>, RepoError>;
    fn update_order_pricing(&self, id: u64, pricing: &PricingResult) -> Result<(), RepoError>;
    fn update_order_status(&self, id: u64, status: Status) -> Result<(), RepoError>;
}

// Notification trait
trait NotificationService {
    fn send_order_confirmation(&self, email: &str) -> Result<(), NotificationError>;
}

// Composed service
struct OrderService<R: OrderRepository, N: NotificationService> {
    repo: R,
    notifier: N,
}

impl<R: OrderRepository, N: NotificationService> OrderService<R, N> {
    fn process_order(&self, order_id: u64) -> Result<(), ServiceError> {
        let order = self.repo.get_order(order_id)?;
        validate_order_status(&order)?;
        
        let items = self.repo.get_order_items(order_id)?;
        let pricing = calculate_pricing(&items);
        
        self.repo.update_order_pricing(order_id, &pricing)?;
        self.repo.update_order_status(order_id, Status::Processed)?;
        self.notifier.send_order_confirmation(&order.customer_email)?;
        
        Ok(())
    }
}
```

## Summary

Always strive to write code that is:
1. **Modular**: Small, focused functions that do one thing
2. **Composable**: Functions that can be combined to build complex behavior
3. **Testable**: Functions that can be tested in isolation without complex setup

Remember: If a function is hard to test, it's probably doing too much!