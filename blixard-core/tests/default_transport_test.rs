//! Test that Iroh is the default transport

use blixard_core::transport::config::TransportConfig;

#[test]
fn test_default_transport_is_iroh() {
    // Test that TransportConfig defaults to Iroh values
    let default_transport = TransportConfig::default();

    // Verify it has expected Iroh relay
    assert_eq!(default_transport.home_relay, "https://relay.iroh.network");
    assert_eq!(default_transport.discovery_port, 0); // Auto-select
    assert!(default_transport.alpn_protocols.is_empty());
}

#[test]
fn test_config_includes_iroh_transport() {
    use blixard_core::config::Config;

    // Test that the main Config includes Iroh transport by default
    let config = Config::default();

    assert!(
        config.transport.is_some(),
        "Config should have transport configured"
    );

    if let Some(transport) = config.transport {
        // Verify it has Iroh configuration
        assert_eq!(transport.home_relay, "https://relay.iroh.network");
        assert_eq!(transport.discovery_port, 0);
        assert!(transport.alpn_protocols.is_empty());
    }
}

#[cfg(test)]
mod transport_config_tests {
    use super::*;
    use blixard_core::transport::config::IrohConfig;

    #[test]
    fn test_iroh_config_defaults() {
        let config = IrohConfig::default();

        // IrohConfig is just a type alias for TransportConfig, so test the same fields
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

        // Should contain the expected relay
        assert!(toml_str.contains("https://relay.iroh.network"));

        // Deserialize back
        let deserialized: TransportConfig =
            toml::from_str(&toml_str).expect("Should deserialize from TOML");

        // Verify the deserialized config has the same values
        assert_eq!(deserialized.home_relay, transport.home_relay);
        assert_eq!(deserialized.discovery_port, transport.discovery_port);
        assert_eq!(deserialized.alpn_protocols, transport.alpn_protocols);
    }
}
