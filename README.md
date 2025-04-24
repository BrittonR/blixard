# Blixard - Distributed Service Management System

![Package Version](https://img.shields.io/hexpm/v/blixard)
![Hex Docs](https://img.shields.io/badge/hex-docs-ffaff3)

Blixard is a distributed service management system built with Gleam and Khepri. It allows for managing systemd services across a cluster of nodes with distributed state management.

## Features

- **Distributed Service Management**: Control systemd services across multiple nodes
- **Reliable State Storage**: Uses Khepri distributed store for service state persistence
- **Automatic Cluster Discovery**: Uses Tailscale for secure node discovery
- **High Availability**: Supports primary and secondary nodes with automatic replication
- **CLI Interface**: Simple command-line interface for service management
- **Reliable Replication**: Continuous monitoring of replication status

## Architecture

Blixard consists of the following core components:

1. **Khepri Store**: Distributed key-value store for service states
2. **Node Manager**: Manages primary and secondary nodes
3. **Service Handlers**: Processes service management operations
4. **Systemd Interface**: Interacts with the systemd service manager
5. **Cluster Discovery**: Discovers and connects to nodes via Tailscale
6. **Replication Monitor**: Monitors replication status between nodes

## Installation

```sh
# Clone the repository
git clone https://github.com/username/blixard.git
cd blixard

# Build the project
gleam build

# Deploy the service manager
sudo ./scripts/deploy.sh
```

## Usage

### Starting the Cluster

Start a primary node:

```sh
start_khepri_node primary
```

Start a secondary node (on another machine):

```sh
start_khepri_node secondary khepri_node@primary-host.local
```

### Managing Services

Start a service:

```sh
service_manager start nginx
```

Stop a service:

```sh
service_manager stop nginx
```

Restart a service:

```sh
service_manager restart nginx
```

Check service status:

```sh
service_manager status nginx
```

List all managed services:

```sh
service_manager list
```

List all connected nodes in the cluster:

```sh
service_manager list-cluster
```

## Configuration

Configuration is managed through environment variables in the `flake.nix` file:

- `BLIXARD_STORAGE_MODE`: Storage mode to use (`khepri` or `ets_dets`)
- `BLIXARD_KHEPRI_OPS`: Comma-separated list of operations to use Khepri for

## Development

```sh
# Run the project
gleam run

# Run the tests
gleam test

# Format the code
gleam format
```

## Project Structure

- `src/`: Source code for the project
  - `blixard.gleam`: Main entry point
  - `service_manager.gleam`: CLI dispatch logic
  - `cli.gleam`: Command-line interface
  - `service_handlers.gleam`: Service management handlers
  - `systemd.gleam`: Systemd service management
  - `khepri_store.gleam`: Khepri distributed store interface
  - `node_manager.gleam`: Node management
  - `cluster_discovery.gleam`: Cluster node discovery
  - `replication_monitor.gleam`: Replication monitoring
- `scripts/`: Deployment and operation scripts
- `test/`: Unit tests

## How It Works

### Cluster Formation

1. A primary node is started with `--init-primary`
2. Secondary nodes are started with `--init-secondary primary_node`
3. Nodes discover each other using Tailscale networking
4. Khepri handles leader election and replication

### Service Management

1. CLI commands are received by `service_manager.gleam`
2. Commands are dispatched to the appropriate handler in `service_handlers.gleam`
3. The handler interacts with systemd via `systemd.gleam`
4. Service state is stored in Khepri via `khepri_store.gleam`
5. State is replicated to all nodes in the cluster

## Further Documentation

For more details, see the module-level and function-level documentation in the source code.
