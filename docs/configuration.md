# Blixard Configuration System

Blixard uses a TOML-based configuration system with support for environment variable overrides and hot-reload of certain settings.

## Configuration Sources

Configuration can be provided through multiple sources, with the following precedence (highest to lowest):

1. **Command-line arguments** - Override all other sources
2. **Environment variables** - Override config file values
3. **Configuration file** (TOML) - Base configuration
4. **Built-in defaults** - Used when no other value is specified

## Using Configuration Files

### Loading a Configuration File

```bash
# Use --config flag to specify a configuration file
blixard --config config/production.toml node --id 1 --bind 0.0.0.0:7001

# Development configuration
blixard --config config/development.toml node --id 1 --bind 127.0.0.1:7001

# Custom configuration
blixard --config /etc/blixard/custom.toml node --id 1 --bind 127.0.0.1:7001
```

### Configuration File Structure

Configuration files are organized into sections:

```toml
[node]
id = 1
bind_address = "127.0.0.1:7001"
data_dir = "./data"
vm_backend = "microvm"
debug = false

[cluster]
bootstrap = false
join_address = "192.168.1.100:7001"

[cluster.raft]
election_tick = 10
heartbeat_tick = 2
# ... more Raft settings

[observability.logging]
level = "info"
format = "json"
# ... more logging settings

# See config/default.toml for all available options
```

## Environment Variable Overrides

### Standard Environment Variables

```bash
# Node configuration
export BLIXARD_NODE_ID=42
export BLIXARD_BIND_ADDRESS="0.0.0.0:8000"
export BLIXARD_DATA_DIR="/var/lib/blixard"

# Cluster configuration
export BLIXARD_JOIN_ADDRESS="192.168.1.100:7001"

# Observability
export BLIXARD_LOG_LEVEL="debug"
export OTEL_EXPORTER_OTLP_ENDPOINT="http://otel-collector:4317"
```

### Legacy Environment Variables

For backward compatibility, the following legacy variables are still supported:

```bash
# Peer connection settings
export BLIXARD_MAX_CONNECTIONS=500
export BLIXARD_CONNECTION_TIMEOUT_MS=10000
export BLIXARD_RECONNECT_DELAY_MS=5000

# Raft settings
export BLIXARD_RAFT_MAX_RESTARTS=10
export BLIXARD_RAFT_RESTART_DELAY_MS=5000

# Worker defaults
export BLIXARD_DEFAULT_CPU_CORES=16
export BLIXARD_DEFAULT_MEMORY_MB=32768
export BLIXARD_DEFAULT_DISK_GB=500
```

## Hot-Reload Support

Certain configuration settings can be changed without restarting the service. The system automatically detects changes to the configuration file and applies them.

### Hot-Reloadable Settings

- `observability.logging.level` - Change log verbosity on the fly
- `observability.logging.format` - Switch between "json" and "pretty" formats
- `cluster.peer.health_check_interval` - Adjust health check frequency
- `cluster.peer.failure_threshold` - Change peer failure detection sensitivity
- `cluster.worker.resource_update_interval` - Modify resource reporting frequency
- `vm.scheduler.default_strategy` - Switch VM placement strategies
- `vm.scheduler.overcommit_ratio` - Adjust resource overcommit allowance
- `network.grpc.keepalive_interval` - Tune connection keepalive
- `network.grpc.keepalive_timeout` - Adjust keepalive timeout

### Settings Requiring Restart

These settings require a full service restart to take effect:

- `node.id` - Node identifier
- `node.bind_address` - gRPC server bind address
- `node.data_dir` - Data storage directory
- `storage.*` - All storage settings
- `network.grpc.port` - gRPC server port
- `security.*` - All security settings

## Example Configurations

### Development Setup

```toml
# config/development.toml
[node]
bind_address = "127.0.0.1:7001"
data_dir = "./dev-data"
vm_backend = "mock"
debug = true

[cluster]
bootstrap = true  # Single-node development

[cluster.raft]
election_tick = 5      # Faster elections for dev
heartbeat_tick = 1

[observability.logging]
level = "debug"
format = "pretty"      # Human-readable logs
```

### Production Multi-Node

```toml
# config/production.toml
[node]
bind_address = "0.0.0.0:7001"
data_dir = "/var/lib/blixard/data"
vm_backend = "microvm"

[cluster]
join_address = "node1.cluster.local:7001"

[cluster.raft]
election_tick = 20     # More conservative for stability
heartbeat_tick = 4

[storage]
sync_mode = "full"     # Maximum durability
compression = true

[observability.logging]
level = "info"
format = "json"
file = "/var/log/blixard/blixard.log"

[security.tls]
enabled = true
cert_file = "/etc/blixard/certs/server.crt"
key_file = "/etc/blixard/certs/server.key"
```

## Configuration Validation

The configuration system validates settings at startup:

```bash
# Validation errors will prevent startup
Error: ConfigError("Heartbeat tick must be less than election tick")
Error: ConfigError("Overcommit ratio must be >= 1.0")
Error: ConfigError("Invalid log level: trace")
```

## Migration from Environment-Only Configuration

If you're migrating from the old environment-based configuration:

1. Generate a base configuration from your environment:
   ```bash
   # Export your current environment variables
   export BLIXARD_MAX_CONNECTIONS=200
   export BLIXARD_CONNECTION_TIMEOUT_MS=5000
   # ... other variables
   
   # Run with --config to use new system
   blixard --config config/migrated.toml node --id 1 --bind 127.0.0.1:7001
   ```

2. The system will automatically apply legacy environment variables to maintain compatibility

3. Gradually move settings from environment variables to your TOML configuration file

## Programmatic Configuration

For integration and testing, configuration can be built programmatically:

```rust
use blixard_core::config_v2::{Config, ConfigBuilder};

let config = ConfigBuilder::new()
    .node_id(42)
    .bind_address("0.0.0.0:8000")
    .data_dir("/var/lib/blixard")
    .join_address("leader.cluster:7001")
    .vm_backend("microvm")
    .log_level("info")
    .build()?;
```

## Best Practices

1. **Use configuration files** for most settings instead of environment variables
2. **Keep secrets separate** - Use environment variables for sensitive data like tokens
3. **Version control** your configuration files (except those with secrets)
4. **Start with examples** - Use provided example configurations as templates
5. **Validate locally** before deploying configuration changes
6. **Monitor hot-reloads** - Check logs when configuration changes are applied
7. **Document customizations** - Add comments to explain non-default settings