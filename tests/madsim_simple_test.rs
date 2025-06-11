// Simple madsim tests that work without complex dependencies

#[cfg(feature = "simulation")]
mod tests {
    use std::time::Duration;

    #[test]
    fn test_basic_madsim_runtime() {
        // Create a madsim runtime
        let rt = madsim::runtime::Runtime::new();
        
        rt.block_on(async {
            // Test basic async operations
            let result = async {
                42
            }.await;
            
            assert_eq!(result, 42);
        });
    }

    #[test]
    fn test_simulated_time() {
        let rt = madsim::runtime::Runtime::new();
        
        rt.block_on(async {
            use madsim::time::{sleep, Instant};
            
            let start = Instant::now();
            
            // Sleep for 1 second (simulated)
            sleep(Duration::from_secs(1)).await;
            
            let elapsed = start.elapsed();
            
            // Should be exactly 1 second in simulated time
            assert!(elapsed >= Duration::from_secs(1));
            assert!(elapsed < Duration::from_millis(1100));
        });
    }

    #[test]
    fn test_deterministic_random() {
        // Test with specific seed
        let rt = madsim::runtime::Runtime::with_seed(42);
        
        let values1 = rt.block_on(async {
            let mut vals = vec![];
            for _ in 0..5 {
                vals.push(madsim::rand::random::<u32>());
            }
            vals
        });
        
        // Same seed should produce same values
        let rt = madsim::runtime::Runtime::with_seed(42);
        
        let values2 = rt.block_on(async {
            let mut vals = vec![];
            for _ in 0..5 {
                vals.push(madsim::rand::random::<u32>());
            }
            vals
        });
        
        assert_eq!(values1, values2);
    }

    #[test]
    fn test_network_simulation_basic() {
        let rt = madsim::runtime::Runtime::new();
        
        rt.block_on(async {
            use madsim::net::NetSim;
            
            let net = NetSim::current();
            
            // Test basic network operations
            let addr1 = "10.0.0.1:8080";
            let addr2 = "10.0.0.2:8080";
            
            // Initially connected
            assert!(net.is_connected(addr1, addr2));
            
            // Disconnect
            net.disconnect(addr1, addr2);
            assert!(!net.is_connected(addr1, addr2));
            
            // Reconnect
            net.connect(addr1, addr2);
            assert!(net.is_connected(addr1, addr2));
        });
    }
}

// Tests that work without the simulation feature
#[test]
fn test_can_compile_without_madsim() {
    // This test ensures the codebase can compile without madsim
    assert_eq!(1 + 1, 2);
}