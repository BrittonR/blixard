{
  description = "A Nix-flake-based Gleam development environment";
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
            gleam 
            erlang 
            rebar3 
            elixir 
            jq
            python3  # For our test HTTP server
          ];
          
          shellHook = ''
            export BLIXARD_STORAGE_MODE="khepri"
            export BLIXARD_KHEPRI_OPS="start,stop,put_vm,get_vm,list_vms,delete_vm,put_host,get_host,list_hosts,delete_host,update_vm_state,assign_vm_to_host"

            echo "BLIXARD_STORAGE_MODE set to: $BLIXARD_STORAGE_MODE"
            
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
            
            # Helper functions
            blixard-build() {
              echo "Building blixard..."
              gleam build
            }
            
            blixard-start-cluster() {
              echo "Starting blixard cluster nodes..."
              
              # Terminal 1: Primary node
              echo "Start primary node in a new terminal with:"
              echo "  gleam run -m service_manager -- --join-cluster"
              
              # Terminal 2: Secondary node  
              echo ""
              echo "Then start secondary node in another terminal with:"
              echo "  gleam run -m service_manager -- --join-cluster khepri_node@127.0.0.1"
            }
            
            blixard-test-service() {
              echo "Testing with HTTP server..."
              
              # Stop any existing instance
              systemctl --user stop test-http-server 2>/dev/null || true
              
              # Start fresh
              gleam run -m service_manager -- start --user test-http-server
              sleep 2
              gleam run -m service_manager -- status --user test-http-server
              
              echo ""
              echo "HTTP server should be running at http://localhost:8888"
              echo "Test with: curl http://localhost:8888"
            }
            
            blixard-test-echo() {
              echo "Testing with echo service..."
              
              gleam run -m service_manager -- start --user test-echo
              sleep 2
              gleam run -m service_manager -- status --user test-echo
              
              echo ""
              echo "Check logs with: journalctl --user -u test-echo -f"
            }
            
            blixard-clean() {
              echo "Cleaning up..."
              systemctl --user stop test-http-server 2>/dev/null || true
              systemctl --user stop test-echo 2>/dev/null || true
              gleam run -m service_manager -- --stop-cluster
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
            
            echo ""
            echo "Blixard Development Environment Ready!"
            echo ""
            echo "Available commands:"
            echo "  blixard-build           - Build the project"
            echo "  blixard-start-cluster   - Instructions to start cluster"
            echo "  blixard-test-service    - Test with HTTP server"
            echo "  blixard-test-echo       - Test with echo service (simpler)"
            echo "  blixard-clean           - Stop services and cluster"
            echo "  blixard-debug-service   - Debug a service (e.g., blixard-debug-service test-http-server)"
            echo ""
            echo "Quick start:"
            echo "  1. Run 'blixard-build'"
            echo "  2. Run 'blixard-start-cluster' and follow instructions"
            echo "  3. Run 'blixard-test-echo' for a simple test"
            echo "  4. Run 'blixard-test-service' for HTTP server test"
          '';
        };
      });
    };
}
