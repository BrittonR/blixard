#!/usr/bin/env bash
# Test script to verify flake-parts template generation

set -euo pipefail

echo "=== Testing Flake-Parts Template Generation ==="

# Create temporary directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "Working directory: $TEMP_DIR"

# Create a simple Rust program to test template rendering
cat > "$TEMP_DIR/test.rs" << 'EOF'
use tera::{Tera, Context};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tera = Tera::default();
    
    // Add the flake-parts template
    let template = include_str!("blixard-vm/nix/templates/vm-flake-parts.nix");
    tera.add_raw_template("vm-flake-parts.nix", template)?;
    
    // Create context
    let mut context = Context::new();
    context.insert("vm_name", "test-vm");
    context.insert("vm_index", &42);
    context.insert("vm_index_hex", "2a");
    context.insert("vm_mac", "02:00:00:00:2a:01");
    context.insert("system", "x86_64-linux");
    context.insert("blixard_modules_path", "/nix/store/fake-modules");
    context.insert("hypervisor", "cloud-hypervisor");
    context.insert("vcpus", &2);
    context.insert("memory", &1024);
    context.insert("flake_modules", &vec!["webserver", "monitoring"]);
    context.insert("file_modules", &vec!["./custom.nix"]);
    context.insert("inline_modules", &vec!["{ services.nginx.enable = true; }"]);
    context.insert("ssh_keys", &vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... test@example"]);
    context.insert("volumes", &vec![
        json!({
            "type": "rootDisk",
            "size": 10240,
        })
    ]);
    
    // Render template
    let output = tera.render("vm-flake-parts.nix", &context)?;
    
    println!("=== Generated Flake ===");
    println!("{}", output);
    
    // Verify key elements
    assert!(output.contains("flake-parts.lib.mkFlake"));
    assert!(output.contains("perSystem"));
    assert!(output.contains("blixard-modules.nixosModules.webserver"));
    assert!(output.contains("blixard-modules.nixosModules.monitoring"));
    assert!(output.contains("./custom.nix"));
    assert!(output.contains("{ services.nginx.enable = true; }"));
    assert!(output.contains("vcpu = 2"));
    assert!(output.contains("mem = 1024"));
    
    println!("\n✅ All checks passed!");
    
    Ok(())
}
EOF

# Run the test from the project root
cd "$(dirname "$0")"
rustc --edition 2021 -L target/debug/deps "$TEMP_DIR/test.rs" -o "$TEMP_DIR/test" \
    --extern tera=$(find target/debug/deps -name "libtera-*.rlib" | head -1) \
    --extern serde_json=$(find target/debug/deps -name "libserde_json-*.rlib" | head -1) \
    2>/dev/null || {
    echo "Note: Direct compilation failed, using cargo to build dependencies..."
    
    # Alternative: create a minimal cargo project
    cd "$TEMP_DIR"
    cargo init --name test-flake-parts
    
    # Add dependencies
    cat > Cargo.toml << 'EOF'
[package]
name = "test-flake-parts"
version = "0.1.0"
edition = "2021"

[dependencies]
tera = "1"
serde_json = "1"
EOF
    
    # Copy template
    mkdir -p blixard-vm/nix/templates
    cp "$OLDPWD/blixard-vm/nix/templates/vm-flake-parts.nix" blixard-vm/nix/templates/
    
    # Copy test source
    cp test.rs src/main.rs
    
    # Build and run
    cargo run
    exit 0
}

# Run the compiled test
"$TEMP_DIR/test"