#[test]
fn test_basic() {
    println!("Basic test runs!");
    assert_eq!(1 + 1, 2);
}

#[cfg(feature = "simulation")]
#[test]
fn test_simulation_feature() {
    println!("Simulation feature is enabled!");
    assert!(true);
}