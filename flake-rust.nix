{
  description = "Blixard - Distributed MicroVM Orchestration Platform";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "nixpkgs/nixos-unstable";
    microvm.url = "github:astro/microvm.nix";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, microvm }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };
        
        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
        
        inputs = with pkgs; [
          # Rust toolchain
          rust
          cargo-nextest
          cargo-watch
          cargo-edit
          
          # Build dependencies
          pkg-config
          openssl.dev
          protobuf
          
          # System dependencies
          systemd.dev
          
          # Storage
          rocksdb
          
          # Networking
          tailscale
          
          # Development tools
          rust-analyzer
          ripgrep
          fd
          
          # Testing
          microvm.packages.${system}.microvm
        ];
      in {
        devShell = pkgs.mkShell {
          packages = inputs;
          
          shellHook = ''
            echo "Blixard Development Environment"
            echo "================================"
            echo "Rust: $(rustc --version)"
            echo "Cargo: $(cargo --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build           - Build the project"
            echo "  cargo nextest run     - Run tests"
            echo "  cargo run -- node     - Start a Blixard node"
            echo "  cargo run -- vm       - VM management commands"
            echo ""
            echo "microvm.nix available at: ${microvm.packages.${system}.microvm}/bin/microvm"
            
            # Create test directories
            mkdir -p /tmp/blixard-test-{1,2,3}/{data,logs}
            
            # Aliases for development
            alias br='cargo build --release'
            alias bt='cargo nextest run'
            alias bw='cargo watch -x "nextest run"'
            alias bts='cargo nextest run --features simulation'
            alias btf='cargo nextest run --features failpoints'
            
            # Run all test types
            test_all() {
              echo "Running unit tests..."
              cargo nextest run
              
              echo "Running property tests..."
              cargo nextest run -p blixard property_test
              
              echo "Running simulation tests..."
              cargo nextest run --features simulation
              
              echo "Running fault injection tests..."
              cargo nextest run --features failpoints
              
              echo "Running model checking tests..."
              cargo nextest run model_checking
            }
            
            # Function to start test nodes
            start_test_cluster() {
              echo "Starting 3-node test cluster..."
              cargo build --release
              
              # Start nodes in background
              ./target/release/blixard node --id 1 --data-dir /tmp/blixard-test-1 &
              ./target/release/blixard node --id 2 --data-dir /tmp/blixard-test-2 --join localhost:7001 &
              ./target/release/blixard node --id 3 --data-dir /tmp/blixard-test-3 --join localhost:7001 &
              
              echo "Cluster started. Use 'pkill blixard' to stop."
            }
          '';
          
          RUST_LOG = "blixard=debug,raft=info";
          RUST_BACKTRACE = "1";
        };
        
        defaultPackage = pkgs.rustPlatform.buildRustPackage {
          pname = "blixard";
          version = "0.1.0";
          
          src = ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          nativeBuildInputs = with pkgs; [
            pkg-config
            protobuf
          ];
          
          buildInputs = with pkgs; [
            openssl
            systemd
          ];
          
          # Tests require microvm.nix
          doCheck = false;
        };
      }
    );
}