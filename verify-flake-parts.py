#!/usr/bin/env python3
"""Verify that the flake-parts template is valid and renders correctly."""

import re

# Read the template
with open("blixard-vm/nix/templates/vm-flake-parts.nix", "r") as f:
    template = f.read()

# Simple template variable substitution for testing
test_vars = {
    "vm_name": "test-vm",
    "vm_index": "42",
    "vm_index_hex": "2a",
    "vm_mac": "02:00:00:00:2a:01",
    "system": "x86_64-linux",
    "blixard_modules_path": "/nix/store/fake-modules",
    "hypervisor": "cloud-hypervisor",
    "vcpus": "2",
    "memory": "1024",
    "flake_modules": ["webserver", "monitoring"],
    "file_modules": ["./custom.nix"],
    "inline_modules": ["{ services.nginx.enable = true; }"],
    "ssh_keys": ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... test@example"],
    "volumes": [{"type": "rootDisk", "size": 10240}],
    "init_command": None,
    "kernel_cmdline": None,
}

# Replace simple variables
output = template
for key, value in test_vars.items():
    if isinstance(value, str):
        output = output.replace("{{ " + key + " }}", value)
    elif isinstance(value, list) and key == "ssh_keys":
        # Handle SSH keys
        ssh_keys_str = "\n".join(f'                  "{k}"' for k in value)
        output = re.sub(
            r'{%- for key in ssh_keys %}\s*"{{ key }}"\s*{%- endfor %}',
            ssh_keys_str,
            output,
        )
    elif value is None:
        # Remove conditional blocks for None values
        output = re.sub(
            rf"{{% if {key} %}}.*?{{% endif %}}", "", output, flags=re.DOTALL
        )

# Handle loops - simplified for demonstration
# Remove template syntax for verification
output = re.sub(r"{%-?\s*for.*?%}", "", output)
output = re.sub(r"{%-?\s*endfor\s*%}", "", output)
output = re.sub(r"{%-?\s*if.*?%}", "", output)
output = re.sub(r"{%-?\s*endif\s*%}", "", output)
output = re.sub(r"{{.*?}}", "TEMPLATE_VAR", output)

print("=== Flake-Parts Template Verification ===\n")

# Check for key flake-parts elements
checks = [
    ("flake-parts import", "flake-parts.url ="),
    ("mkFlake function", "flake-parts.lib.mkFlake"),
    ("perSystem", "perSystem ="),
    ("nixosConfigurations", 'nixosConfigurations."test-vm"'),
    ("microvm config", "microvm ="),
    ("hypervisor setting", 'hypervisor = "cloud-hypervisor"'),
    ("vcpu setting", "vcpu = 2"),
    ("memory setting", "mem = 1024"),
    ("blixard modules", "blixard-modules"),
    ("SSH configuration", "services.openssh"),
    ("Network configuration", "systemd.network.networks"),
    ("Console socket", 'socket = "/tmp/test-vm-console.sock"'),
    ("Apps definition", "apps.TEMPLATE_VAR"),
    ("Dev shell", "devShells.TEMPLATE_VAR"),
]

all_passed = True
for check_name, pattern in checks:
    if pattern in output:
        print(f"✅ {check_name}")
    else:
        print(f"❌ {check_name} - pattern '{pattern}' not found")
        all_passed = False

print("\n=== Template Structure ===")
print("Lines:", len(output.split("\n")))
print("Size:", len(output), "bytes")

# Show first 30 lines
print("\n=== First 30 lines of rendered template ===")
for i, line in enumerate(output.split("\n")[:30]):
    print(f"{i+1:3}: {line}")

if all_passed:
    print("\n✅ All template checks passed!")
else:
    print("\n❌ Some checks failed!")
    exit(1)
