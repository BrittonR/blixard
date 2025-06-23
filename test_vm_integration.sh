#!/usr/bin/env bash

echo "=== Testing Blixard VM Integration ==="
echo

# Clean up from previous runs
rm -rf test-vm-configs test-vm-data generated-flakes

echo "1. Running unit tests..."
cargo test -p blixard-vm --lib
echo

echo "2. Running integration tests..."
cargo test -p blixard-vm --test '*'
echo

echo "3. Generating a VM flake..."
cargo run -p blixard-vm --example generate_vm_flake
echo

echo "4. Checking generated flake..."
if [ -f "generated-flakes/example-vm/flake.nix" ]; then
    echo "✓ Flake generated successfully!"
    echo "First 20 lines of generated flake:"
    head -20 generated-flakes/example-vm/flake.nix
else
    echo "✗ Flake not found!"
fi
echo

echo "5. Running VM lifecycle example..."
cargo run -p blixard-vm --example vm_lifecycle
echo

echo "=== Summary ==="
echo "The microvm.nix integration provides:"
echo "- Nix flake generation for VM configurations"
echo "- Process management for VM lifecycle"
echo "- Integration with the VmBackend trait"
echo
echo "To actually run VMs, you would need:"
echo "1. Nix with flakes enabled"
echo "2. microvm.nix installed"
echo "3. A hypervisor (cloud-hypervisor or firecracker)"
echo "4. Proper networking setup (bridge interfaces)"
echo
echo "The generated flakes would be built with:"
echo "  nix build ./generated-flakes/example-vm#nixosConfigurations.example-vm.config.microvm.runner"