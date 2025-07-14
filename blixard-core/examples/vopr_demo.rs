//! VOPR Fuzzer Demo
//!
//! This example demonstrates how to run the VOPR fuzzer to test
//! Blixard's distributed consensus system.

use blixard_core::vopr::{Vopr, VoprConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 Blixard VOPR Fuzzer Demo");
    println!("=============================\n");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let seed = if args.len() > 1 {
        args[1].parse::<u64>().unwrap_or(42)
    } else {
        42
    };

    println!("Configuration:");
    println!("  Seed: {}", seed);
    println!("  Time acceleration: 1000x");
    println!("  Max operations: 1000");
    println!("  Coverage-guided: enabled");
    println!("  Shrinking: enabled\n");

    // Create fuzzer configuration
    let config = VoprConfig {
        seed,
        time_acceleration: 1000,
        max_clock_skew_ms: 5000,
        check_liveness: true,
        check_safety: true,
        max_operations: 1000,
        enable_shrinking: true,
        enable_visualization: true,
        coverage_guided: true,
    };

    // Create and run the fuzzer
    let mut vopr = Vopr::new(config);

    match vopr.run().await {
        Ok(()) => {
            println!("\n✅ Fuzzing completed successfully!");
            println!("No invariant violations found.");
        }
        Err(failure) => {
            println!("\n❌ FUZZING FAILURE!");
            println!("==================\n");

            // Print failure details
            println!("Violated invariant: {}", failure.violated_invariant);
            println!("Seed: {}", failure.seed);
            println!(
                "Original test case: {} operations",
                failure.operations.len()
            );

            if let Some(minimal) = &failure.minimal_reproducer {
                println!("Minimized to: {} operations", minimal.len());
            }

            // Generate and print the report
            println!("\nDetailed Report:");
            println!("================");
            let report = vopr.generate_report(&failure);
            println!("{}", report);

            // Generate reproducer code
            println!("\nReproducer Code:");
            println!("================");
            let reproducer = vopr.generate_reproducer(&failure);
            println!("{}", reproducer);

            // Save to file
            let filename = format!("vopr_failure_{}.txt", failure.seed);
            std::fs::write(&filename, &report)?;
            println!("\nReport saved to: {}", filename);

            return Err("Invariant violation detected".into());
        }
    }

    Ok(())
}
