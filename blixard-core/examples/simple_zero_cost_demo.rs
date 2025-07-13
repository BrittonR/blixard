//! Simple demonstration of zero-cost abstractions
//!
//! This example shows the basic patterns without complex dependencies.

use blixard_core::zero_cost::simple_demo::*;

fn main() -> Result<(), &'static str> {
    println!("🚀 Simple Zero-Cost Abstractions Demo\n");

    demonstrate_type_state()?;
    println!();

    demonstrate_validated_types()?;
    println!();

    demonstrate_phantom_types()?;
    println!();

    demonstrate_zero_allocation()?;
    println!();

    demonstrate_const_collections()?;
    println!();

    println!("✅ All demonstrations completed!");
    println!("\n📈 Key Benefits Demonstrated:");
    println!("  • Type-state pattern prevents invalid state transitions");
    println!("  • Validated types ensure data integrity at compile time");
    println!("  • Phantom types provide unit safety with zero overhead");
    println!("  • Zero-allocation patterns eliminate heap usage");
    println!("  • Const collections provide fast compile-time lookups");
    
    Ok(())
}