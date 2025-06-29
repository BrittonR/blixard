//! Test that Iroh is the default transport

use blixard_core::transport::config::{TransportConfig, RaftTransportPreference};

#[test]
fn test_default_transport_is_iroh() {
    // Test that TransportConfig defaults to Iroh
    let default_transport = TransportConfig::default();
    
    match default_transport {
        TransportConfig::Iroh(_) => {
            // This is expected - Iroh is the default
        }
        _ => panic!("Expected default transport to be Iroh, but got {:?}", default_transport),
    }
}

#[test]
fn test_default_raft_preference_is_iroh() {
    // Test that RaftTransportPreference defaults to AlwaysIroh
    let default_pref = RaftTransportPreference::default();
    
    match default_pref {
        RaftTransportPreference::AlwaysIroh => {
            // This is expected
        }
        _ => panic!("Expected default Raft preference to be AlwaysIroh, but got {:?}", default_pref),
    }
}

#[test]
fn test_config_includes_iroh_transport() {
    use blixard_core::config_v2::Config;
    
    // Test that the main Config includes Iroh transport by default
    let config = Config::default();
    
    assert!(config.transport.is_some(), "Config should have transport configured");
    
    if let Some(transport) = config.transport {
        match transport {
            TransportConfig::Iroh(_) => {
                // This is expected
            }
            _ => panic!("Expected Config to have Iroh transport by default, but got {:?}", transport),
        }
    }
}

#[cfg(test)]
mod transport_config_tests {
    use super::*;
    use blixard_core::transport::config::IrohConfig;
    
    #[test]
    fn test_iroh_config_defaults() {
        let config = IrohConfig::default();
        
        assert!(config.enabled);
        assert_eq!(config.home_relay, "https://relay.iroh.network");
        assert_eq!(config.discovery_port, 0); // Auto-select
        assert!(config.alpn_protocols.is_empty());
    }
    
    #[test]
    fn test_transport_serialization() {
        // Test that transport config can be serialized/deserialized
        let transport = TransportConfig::default();
        
        // Serialize to TOML
        let toml_str = toml::to_string(&transport).expect("Should serialize to TOML");
        
        // Should contain mode = "iroh"
        assert!(toml_str.contains("mode = \"iroh\""));
        
        // Deserialize back
        let deserialized: TransportConfig = toml::from_str(&toml_str)
            .expect("Should deserialize from TOML");
        
        match deserialized {
            TransportConfig::Iroh(_) => {
                // Good, still Iroh
            }
            _ => panic!("Deserialized transport should be Iroh"),
        }
    }
}