{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    blixard-modules = {
      url = "path:{{ modules_path }}";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.microvm.flakeModule
        inputs.blixard-modules.flakeModule
      ];
      
      systems = [ "{{ system }}" ];
      
      perSystem = { config, pkgs, ... }: {
        microvm.vms = {
          "{{ vm_name }}" = {
            inherit pkgs;
            imports = [
              {% for import in imports %}
              {{ import }}
              {% endfor %}
              ({ ... }: {
                # VM-specific configuration
                microvm = {
                  hypervisor = "{{ hypervisor }}";
                  vcpu = {{ vcpus }};
                  mem = {{ memory }};
                  
                  {% if networks %}
                  interfaces = [
                    {% for network in networks %}
                    {
                      type = "{{ network.type }}";
                      id = "{{ network.name }}";
                      {% if network.bridge %}bridge = "{{ network.bridge }}";{% endif %}
                      {% if network.mac %}mac = "{{ network.mac }}";{% endif %}
                    }
                    {% endfor %}
                  ];
                  {% endif %}
                  
                  {% if volumes %}
                  volumes = [
                    {% for volume in volumes %}
                    {
                      {% if volume.type == "rootDisk" %}
                      image = "rootdisk.img";
                      size = {{ volume.size }};
                      {% elif volume.type == "dataDisk" %}
                      image = "{{ volume.path }}";
                      size = {{ volume.size }};
                      readOnly = {{ volume.readOnly | lower }};
                      {% elif volume.type == "virtiofs" %}
                      tag = "{{ volume.tag }}";
                      source = "{{ volume.path }}";
                      mountPoint = "{{ volume.mountPoint }}";
                      {% endif %}
                    }
                    {% endfor %}
                  ];
                  {% endif %}
                };
                
                # Basic NixOS configuration
                services.getty.autologinUser = "root";
                
                {% if init_command %}
                systemd.services.init-command = {
                  description = "Run initialization command";
                  wantedBy = [ "multi-user.target" ];
                  after = [ "network.target" ];
                  serviceConfig = {
                    Type = "oneshot";
                    ExecStart = "{{ init_command }}";
                    RemainAfterExit = true;
                  };
                };
                {% endif %}
              })
            ];
          };
        };
      };
    };
}