{
  description = "A Nix-flake-based Rust and Gleam development environment for Blixard";
  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1";
  
  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSupportedSystem = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
        pkgs = import nixpkgs { inherit system; };
      });
    in
    {
      devShells = forEachSupportedSystem ({ pkgs }: {
        default = pkgs.mkShell {
          packages = with pkgs; [ 
            # Rust development
            rustc
            cargo
            rust-analyzer
            rustfmt
            clippy
            
            # Build tools
            pkg-config
            openssl
            protobuf
            
            # Debugging and profiling
            gdb
            valgrind
            perf-tools
            
            # Gleam development (keeping for reference/comparison)
            gleam 
            erlang 
            rebar3 
            elixir 
            
            # Testing and utilities
            jq
            python3  # For test HTTP server
            zellij   # For interactive testing environment
            curl     # For testing HTTP endpoints
            netcat   # For port checking
            systemd  # For systemctl commands
          ];
          
          shellHook = ''
            # Environment variables for Rust project
            export RUST_BACKTRACE=1
            export RUST_LOG=blixard=debug,tikv_raft=info
            
            # Keep existing BLIXARD settings for compatibility
            export BLIXARD_STORAGE_MODE="redb"
            export BLIXARD_CONFIG_PATH="$HOME/.config/blixard/config.toml"

            echo "Blixard Rust Development Environment"
            echo "===================================="
            
            # Create systemd user service for testing with proper python path
            mkdir -p ~/.config/systemd/user/
            
            # Find the actual python3 path in nix store
            PYTHON3_PATH=$(which python3)
            echo "Using python3 from: $PYTHON3_PATH"
            
            cat > ~/.config/systemd/user/test-http-server.service << EOF
            [Unit]
            Description=Test HTTP Server for Blixard
            After=network.target
            
            [Service]
            Type=simple
            ExecStart=$PYTHON3_PATH -m http.server 8888 --directory /tmp
            WorkingDirectory=/tmp
            Restart=on-failure
            RestartSec=5
            StandardOutput=journal
            StandardError=journal
            
            [Install]
            WantedBy=default.target
            EOF
            
            # Also create a simple echo service for testing
            cat > ~/.config/systemd/user/test-echo.service << EOF
            [Unit]
            Description=Test Echo Service for Blixard
            
            [Service]
            Type=simple
            ExecStart=/bin/sh -c 'while true; do echo "Echo service running at $(date)"; sleep 10; done'
            StandardOutput=journal
            StandardError=journal
            
            [Install]
            WantedBy=default.target
            EOF
            
            # Reload systemd user daemon
            systemctl --user daemon-reload
            
            # Rust-specific helper functions
            blixard-build() {
              echo "Building Blixard (Rust)..."
              cargo build --release
            }
            
            blixard-dev() {
              echo "Building Blixard in debug mode..."
              cargo build
            }
            
            blixard-test() {
              echo "Running tests..."
              cargo test
            }
            
            blixard-run() {
              echo "Running Blixard..."
              cargo run -- "$@"
            }
            
            blixard-clippy() {
              echo "Running Clippy lints..."
              cargo clippy -- -D warnings
            }
            
            blixard-fmt() {
              echo "Formatting code..."
              cargo fmt
            }
            
            blixard-bench() {
              echo "Running benchmarks..."
              cargo bench
            }
            
            blixard-install() {
              echo "Installing Blixard to ~/.cargo/bin..."
              cargo install --path .
            }
            
            # Service testing functions
            blixard-start-primary() {
              echo "Starting primary cluster node..."
              cargo run -- init-primary
            }
            
            blixard-start-secondary() {
              echo "Starting secondary cluster node..."
              cargo run -- init-secondary --primary-addr 127.0.0.1:9090
            }
            
            blixard-test-service() {
              echo "Testing with HTTP server..."
              
              # Stop any existing instance
              systemctl --user stop test-http-server 2>/dev/null || true
              
              # Start fresh
              cargo run -- start test-http-server --user
              sleep 2
              cargo run -- status test-http-server --user
              
              echo ""
              echo "HTTP server should be running at http://localhost:8888"
              echo "Test with: curl http://localhost:8888"
            }
            
            blixard-test-echo() {
              echo "Testing with echo service..."
              
              cargo run -- start test-echo --user
              sleep 2
              cargo run -- status test-echo --user
            }
            
            blixard-list() {
              echo "Listing services..."
              cargo run -- list
            }
            
            blixard-cluster-status() {
              echo "Checking cluster status..."
              cargo run -- cluster-status
            }
            
            blixard-clean() {
              echo "Cleaning up..."
              systemctl --user stop test-http-server 2>/dev/null || true
              systemctl --user stop test-echo 2>/dev/null || true
              rm -rf ~/.local/share/blixard
              rm -rf ~/.config/blixard
              echo "Cleanup complete"
            }
            
            blixard-logs() {
              echo "=== Recent Service Logs ==="
              echo ""
              echo "Test HTTP Server:"
              journalctl --user -u test-http-server -n 10 --no-pager
              echo ""
              echo "Test Echo Service:"
              journalctl --user -u test-echo -n 10 --no-pager
            }
            
            blixard-debug-service() {
              SERVICE=$1
              echo "Debugging service: $SERVICE"
              echo ""
              echo "Service file location:"
              systemctl --user cat $SERVICE 2>/dev/null || echo "Service not found"
              echo ""
              echo "Service status:"
              systemctl --user status $SERVICE --no-pager
              echo ""
              echo "Recent logs:"
              journalctl --user -u $SERVICE -n 20 --no-pager
            }
            
            # Gleam compatibility functions (for reference)
            blixard-gleam-build() {
              echo "Building Gleam version..."
              gleam build
            }
            
            blixard-gleam-run() {
              echo "Running Gleam version..."
              gleam run -m service_manager -- "$@"
            }
            
            echo ""
            echo "Blixard Rust Development Environment Ready!"
            echo ""
            echo "Build & Development:"
            echo "  blixard-build           - Build release version"
            echo "  blixard-dev             - Build debug version"
            echo "  blixard-test            - Run tests"
            echo "  blixard-run [args]      - Run with cargo"
            echo "  blixard-clippy          - Run linter"
            echo "  blixard-fmt             - Format code"
            echo "  blixard-bench           - Run benchmarks"
            echo "  blixard-install         - Install to ~/.cargo/bin"
            echo ""
            echo "Cluster Operations:"
            echo "  blixard-start-primary   - Start primary node"
            echo "  blixard-start-secondary - Start secondary node"
            echo "  blixard-cluster-status  - Check cluster status"
            echo ""
            echo "Service Management:"
            echo "  blixard-test-service    - Test with HTTP server"
            echo "  blixard-test-echo       - Test with echo service"
            echo "  blixard-list            - List all services"
            echo "  blixard-clean           - Clean up everything"
            echo "  blixard-logs            - Show service logs"
            echo "  blixard-debug-service   - Debug a specific service"
            echo ""
            echo "Gleam Reference (old):"
            echo "  blixard-gleam-build     - Build Gleam version"
            echo "  blixard-gleam-run       - Run Gleam version"
          '';
        };
      });
    };
}