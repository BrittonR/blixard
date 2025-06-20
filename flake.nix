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
            echo "  cargo nt-all          - Run all tests (main workspace)"
            echo "  cargo run -- node     - Start a Blixard node"
            echo "  cargo run -- vm       - VM management commands"
            echo ""
            echo "MadSim tests (deterministic distributed systems testing):"
            echo "  mnt-all               - Run all MadSim tests"
            echo "  mnt-byzantine         - Run Byzantine failure tests"  
            echo "  mnt-clock-skew        - Run clock skew tests"
            echo "  madsim-all            - Alternative name for mnt-all"
            echo ""
            echo "Test scripts:"
            echo "  ./scripts/test-madsim.sh all      - MadSim tests (alternative)"
            echo "  ./quick_test.sh                   - Quick 2-node test"
            echo "  ./test_bootstrap.sh               - 3-node bootstrap test"
            echo ""
            echo "microvm.nix available at: ${microvm.packages.${system}.microvm}/bin/microvm"
            
            # Create test directories
            mkdir -p test-data/node{1,2,3}
            
            # Aliases for development
            alias br='cargo build --release'
            alias bt='cargo test'
            alias bw='cargo watch -x test'
            alias bts='cargo test --features simulation'
            alias btf='cargo test --features failpoints'
            
            # MadSim test functions (auto-set RUSTFLAGS)
            mnt-all() {
                RUSTFLAGS="--cfg madsim" cargo nt-madsim "$@"
            }
            
            mnt-byzantine() {
                RUSTFLAGS="--cfg madsim" cargo nt-byzantine "$@"
            }
            
            mnt-clock-skew() {
                RUSTFLAGS="--cfg madsim" cargo nt-clock-skew "$@"
            }
            
            # Alternative shorter names
            madsim-all() {
                RUSTFLAGS="--cfg madsim" cargo nt-madsim "$@"
            }
            
            madsim-byzantine() {
                RUSTFLAGS="--cfg madsim" cargo nt-byzantine "$@"
            }
            
            madsim-clock-skew() {
                RUSTFLAGS="--cfg madsim" cargo nt-clock-skew "$@"
            }
            
            # Run all test types
            test_all() {
              echo "Running unit tests..."
              cargo test
              
              echo "Running integration tests..."
              cargo test --test '*'
              
              echo "Running simulation tests..."
              cargo test --features simulation
              
              echo "Running fault injection tests..."
              cargo test --features failpoints
            }
            
            # Function to start test nodes
            start_test_cluster() {
              echo "Starting 3-node test cluster..."
              cargo build --release
              
              # Start nodes in background
              ./target/release/blixard node --id 1 --bind 127.0.0.1:7001 &
              ./target/release/blixard node --id 2 --bind 127.0.0.1:7002 --peer 1:127.0.0.1:7001 &
              ./target/release/blixard node --id 3 --bind 127.0.0.1:7003 --peer 1:127.0.0.1:7001 --peer 2:127.0.0.1:7002 &
              
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