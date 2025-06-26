use blixard_core::config_v2::{Config, ConfigBuilder};
use std::fs;
use std::time::Duration;
use std::path::PathBuf;
use tempfile::NamedTempFile;

#[test]
fn test_default_config() {
    let config = Config::default();
    
    // Check some defaults
    assert_eq!(config.node.bind_address, "127.0.0.1:7001");
    assert_eq!(config.node.vm_backend, "mock");
    assert!(!config.node.debug);
    assert_eq!(config.cluster.peer.max_connections, 100);
    assert_eq!(config.cluster.raft.election_tick, 10);
}

#[test]
fn test_config_builder() {
    let config = ConfigBuilder::new()
        .node_id(42)
        .bind_address("192.168.1.100:8000")
        .data_dir("/var/lib/blixard")
        .join_address("192.168.1.1:8000")
        .vm_backend("microvm")
        .log_level("debug")
        .build()
        .unwrap();
    
    assert_eq!(config.node.id, Some(42));
    assert_eq!(config.node.bind_address, "192.168.1.100:8000");
    assert_eq!(config.node.data_dir.to_str().unwrap(), "/var/lib/blixard");
    assert_eq!(config.cluster.join_address, Some("192.168.1.1:8000".to_string()));
    assert_eq!(config.node.vm_backend, "microvm");
    assert_eq!(config.observability.logging.level, "debug");
}

#[test]
fn test_load_from_toml() {
    let toml_content = r#"
[node]
id = 1
bind_address = "0.0.0.0:7001"
data_dir = "/data"
vm_backend = "docker"
debug = true

[cluster.raft]
election_tick = 20
heartbeat_tick = 5
conf_change_timeout = "10s"

[cluster.peer]
max_connections = 200
connection_timeout = "3s"

[observability.logging]
level = "warn"
format = "json"
"#;
    
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();
    
    let config = Config::from_file(temp_file.path()).unwrap();
    
    assert_eq!(config.node.id, Some(1));
    assert_eq!(config.node.bind_address, "0.0.0.0:7001");
    assert_eq!(config.node.data_dir.to_str().unwrap(), "/data");
    assert_eq!(config.node.vm_backend, "docker");
    assert!(config.node.debug);
    
    assert_eq!(config.cluster.raft.election_tick, 20);
    assert_eq!(config.cluster.raft.heartbeat_tick, 5);
    assert_eq!(config.cluster.raft.conf_change_timeout, Duration::from_secs(10));
    
    assert_eq!(config.cluster.peer.max_connections, 200);
    assert_eq!(config.cluster.peer.connection_timeout, Duration::from_secs(3));
    
    assert_eq!(config.observability.logging.level, "warn");
    assert_eq!(config.observability.logging.format, "json");
}

#[test]
fn test_partial_config() {
    // Test that partial configs work with defaults
    let toml_content = r#"
[node]
bind_address = "192.168.0.1:9000"

[observability.logging]
level = "debug"
"#;
    
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();
    
    let config = Config::from_file(temp_file.path()).unwrap();
    
    // Specified values
    assert_eq!(config.node.bind_address, "192.168.0.1:9000");
    assert_eq!(config.observability.logging.level, "debug");
    
    // Default values
    assert_eq!(config.node.vm_backend, "mock");
    assert_eq!(config.cluster.peer.max_connections, 100);
}

#[test]
fn test_environment_override() {
    std::env::set_var("BLIXARD_NODE_ID", "99");
    std::env::set_var("BLIXARD_BIND_ADDRESS", "10.0.0.1:8080");
    std::env::set_var("BLIXARD_LOG_LEVEL", "trace");
    
    let config = Config::default();
    let mut config_with_env = config.clone();
    config_with_env.apply_env_overrides();
    
    assert_eq!(config_with_env.node.id, Some(99));
    assert_eq!(config_with_env.node.bind_address, "10.0.0.1:8080");
    assert_eq!(config_with_env.observability.logging.level, "trace");
    
    // Clean up
    std::env::remove_var("BLIXARD_NODE_ID");
    std::env::remove_var("BLIXARD_BIND_ADDRESS");
    std::env::remove_var("BLIXARD_LOG_LEVEL");
}

#[test]
fn test_config_validation() {
    let mut config = Config::default();
    
    // Valid config should pass
    assert!(config.validate().is_ok());
    
    // Empty bind address should fail
    config.node.bind_address = "".to_string();
    assert!(config.validate().is_err());
    config.node.bind_address = "127.0.0.1:7001".to_string();
    
    // Invalid Raft config should fail
    config.cluster.raft.heartbeat_tick = 20;
    config.cluster.raft.election_tick = 10;
    assert!(config.validate().is_err());
    config.cluster.raft.heartbeat_tick = 2;
    
    // Invalid overcommit ratio should fail
    config.vm.scheduler.overcommit_ratio = 0.5;
    assert!(config.validate().is_err());
    config.vm.scheduler.overcommit_ratio = 1.2;
    
    // Invalid log level should fail
    config.observability.logging.level = "invalid".to_string();
    assert!(config.validate().is_err());
}

#[test]
fn test_hot_reloadable_detection() {
    assert!(Config::is_hot_reloadable("observability.logging.level"));
    assert!(Config::is_hot_reloadable("cluster.peer.health_check_interval"));
    assert!(Config::is_hot_reloadable("vm.scheduler.default_strategy"));
    
    assert!(!Config::is_hot_reloadable("node.bind_address"));
    assert!(!Config::is_hot_reloadable("node.id"));
    assert!(!Config::is_hot_reloadable("storage.db_name"));
}


#[test]
fn test_duration_serialization() {
    let toml_content = r#"
[cluster.raft]
conf_change_timeout = "5s"
proposal_timeout = "1m30s"
restart_delay = "500ms"

[cluster.peer]
connection_timeout = "10s"
reconnect_delay = "2s500ms"
health_check_interval = "30s"
"#;
    
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();
    
    let config = Config::from_file(temp_file.path()).unwrap();
    
    assert_eq!(config.cluster.raft.conf_change_timeout, Duration::from_secs(5));
    assert_eq!(config.cluster.raft.proposal_timeout, Duration::from_secs(90));
    assert_eq!(config.cluster.raft.restart_delay, Duration::from_millis(500));
    
    assert_eq!(config.cluster.peer.connection_timeout, Duration::from_secs(10));
    assert_eq!(config.cluster.peer.reconnect_delay, Duration::from_millis(2500));
    assert_eq!(config.cluster.peer.health_check_interval, Duration::from_secs(30));
}

#[test]
fn test_security_config_defaults() {
    let config = Config::default();
    
    // TLS should be disabled by default
    assert!(!config.security.tls.enabled);
    assert!(config.security.tls.cert_file.is_none());
    assert!(config.security.tls.key_file.is_none());
    assert!(config.security.tls.ca_file.is_none());
    assert!(!config.security.tls.require_client_cert);
    
    // Auth should be disabled by default
    assert!(!config.security.auth.enabled);
    assert_eq!(config.security.auth.method, "token");
    assert!(config.security.auth.token_file.is_none());
}

#[test]
fn test_security_config_from_toml() {
    let toml_content = r#"
[security.tls]
enabled = true
cert_file = "/etc/blixard/certs/server.crt"
key_file = "/etc/blixard/certs/server.key"
ca_file = "/etc/blixard/certs/ca.crt"
require_client_cert = true

[security.auth]
enabled = true
method = "mtls"
token_file = "/etc/blixard/tokens/auth.token"
"#;
    
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();
    
    let config = Config::from_file(temp_file.path()).unwrap();
    
    // TLS configuration
    assert!(config.security.tls.enabled);
    assert_eq!(config.security.tls.cert_file, Some(PathBuf::from("/etc/blixard/certs/server.crt")));
    assert_eq!(config.security.tls.key_file, Some(PathBuf::from("/etc/blixard/certs/server.key")));
    assert_eq!(config.security.tls.ca_file, Some(PathBuf::from("/etc/blixard/certs/ca.crt")));
    assert!(config.security.tls.require_client_cert);
    
    // Auth configuration
    assert!(config.security.auth.enabled);
    assert_eq!(config.security.auth.method, "mtls");
    assert_eq!(config.security.auth.token_file, Some(PathBuf::from("/etc/blixard/tokens/auth.token")));
}

#[test]
fn test_security_config_partial() {
    let toml_content = r#"
[security.tls]
enabled = true
cert_file = "/path/to/cert.pem"
key_file = "/path/to/key.pem"
# ca_file and require_client_cert should use defaults

[security.auth]
enabled = true
# method and token_file should use defaults
"#;
    
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();
    
    let config = Config::from_file(temp_file.path()).unwrap();
    
    // TLS configuration
    assert!(config.security.tls.enabled);
    assert_eq!(config.security.tls.cert_file, Some(PathBuf::from("/path/to/cert.pem")));
    assert_eq!(config.security.tls.key_file, Some(PathBuf::from("/path/to/key.pem")));
    assert!(config.security.tls.ca_file.is_none()); // Default
    assert!(!config.security.tls.require_client_cert); // Default
    
    // Auth configuration
    assert!(config.security.auth.enabled);
    assert_eq!(config.security.auth.method, "token"); // Default
    assert!(config.security.auth.token_file.is_none()); // Default
}

#[test]
fn test_security_config_builder() {
    let config = ConfigBuilder::new()
        .security_tls_enabled(true)
        .security_tls_cert("/custom/cert.pem")
        .security_tls_key("/custom/key.pem")
        .security_auth_enabled(true)
        .security_auth_method("token")
        .build()
        .unwrap();
    
    // TLS configuration
    assert!(config.security.tls.enabled);
    assert_eq!(config.security.tls.cert_file, Some(PathBuf::from("/custom/cert.pem")));
    assert_eq!(config.security.tls.key_file, Some(PathBuf::from("/custom/key.pem")));
    
    // Auth configuration
    assert!(config.security.auth.enabled);
    assert_eq!(config.security.auth.method, "token");
}

#[test]
fn test_production_security_config() {
    // Test loading the production config which has security enabled
    let production_config_path = "../config/production.toml";
    
    // Skip test if production config doesn't exist
    if !std::path::Path::new(production_config_path).exists() {
        return;
    }
    
    let config = Config::from_file(production_config_path).unwrap();
    
    // Production should have security enabled
    assert!(config.security.tls.enabled);
    assert!(config.security.auth.enabled);
    assert_eq!(config.security.auth.method, "mtls");
    assert!(config.security.tls.require_client_cert);
}

#[test]
fn test_security_environment_overrides() {
    // Set up environment variables
    std::env::set_var("BLIXARD_TLS_ENABLED", "true");
    std::env::set_var("BLIXARD_TLS_CERT_FILE", "/env/cert.pem");
    std::env::set_var("BLIXARD_TLS_KEY_FILE", "/env/key.pem");
    std::env::set_var("BLIXARD_TLS_CA_FILE", "/env/ca.pem");
    std::env::set_var("BLIXARD_TLS_REQUIRE_CLIENT_CERT", "true");
    std::env::set_var("BLIXARD_AUTH_ENABLED", "true");
    std::env::set_var("BLIXARD_AUTH_METHOD", "mtls");
    std::env::set_var("BLIXARD_AUTH_TOKEN_FILE", "/env/tokens.json");
    
    let mut config = Config::default();
    config.apply_env_overrides();
    
    // TLS configuration from environment
    assert!(config.security.tls.enabled);
    assert_eq!(config.security.tls.cert_file, Some(PathBuf::from("/env/cert.pem")));
    assert_eq!(config.security.tls.key_file, Some(PathBuf::from("/env/key.pem")));
    assert_eq!(config.security.tls.ca_file, Some(PathBuf::from("/env/ca.pem")));
    assert!(config.security.tls.require_client_cert);
    
    // Auth configuration from environment
    assert!(config.security.auth.enabled);
    assert_eq!(config.security.auth.method, "mtls");
    assert_eq!(config.security.auth.token_file, Some(PathBuf::from("/env/tokens.json")));
    
    // Clean up environment variables
    std::env::remove_var("BLIXARD_TLS_ENABLED");
    std::env::remove_var("BLIXARD_TLS_CERT_FILE");
    std::env::remove_var("BLIXARD_TLS_KEY_FILE");
    std::env::remove_var("BLIXARD_TLS_CA_FILE");
    std::env::remove_var("BLIXARD_TLS_REQUIRE_CLIENT_CERT");
    std::env::remove_var("BLIXARD_AUTH_ENABLED");
    std::env::remove_var("BLIXARD_AUTH_METHOD");
    std::env::remove_var("BLIXARD_AUTH_TOKEN_FILE");
}

#[test]
fn test_security_config_validation() {
    let mut config = Config::default();
    
    // Valid default config should pass
    assert!(config.validate().is_ok());
    
    // TLS enabled without certificates should fail
    config.security.tls.enabled = true;
    assert!(config.validate().is_err());
    
    // Add cert file but not key file should still fail
    config.security.tls.cert_file = Some(PathBuf::from("/path/to/cert.pem"));
    assert!(config.validate().is_err());
    
    // Add key file should make it valid
    config.security.tls.key_file = Some(PathBuf::from("/path/to/key.pem"));
    assert!(config.validate().is_ok());
    
    // Invalid auth method should fail
    config.security.auth.enabled = true;
    config.security.auth.method = "invalid".to_string();
    assert!(config.validate().is_err());
    
    // Valid auth method should pass
    config.security.auth.method = "token".to_string();
    assert!(config.validate().is_ok());
    
    config.security.auth.method = "mtls".to_string();
    assert!(config.validate().is_ok());
}