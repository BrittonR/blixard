{ lib, config, ... }:

let
  inherit (lib) mkOption types;

  # Network configuration type
  networkType = types.submodule {
    options = {
      type = mkOption {
        type = types.enum [ "tap" "user" ];
        default = "tap";
        description = "Network interface type";
      };
      name = mkOption {
        type = types.str;
        description = "Network interface name";
      };
      bridge = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Bridge to attach the TAP interface to";
      };
      mac = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "MAC address for the interface";
      };
    };
  };

  # Volume configuration type
  volumeType = types.submodule {
    options = {
      type = mkOption {
        type = types.enum [ "rootDisk" "dataDisk" "virtiofs" ];
        description = "Volume type";
      };
      size = mkOption {
        type = types.int;
        default = 10240;
        description = "Size in MB (for disk volumes)";
      };
      path = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Path to the disk image or shared directory";
      };
      tag = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Tag for virtiofs shares";
      };
      mountPoint = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Mount point inside the VM (for virtiofs)";
      };
      readOnly = mkOption {
        type = types.bool;
        default = false;
        description = "Whether the volume is read-only";
      };
    };
  };

in
{
  options.blixard.vms = mkOption {
    type = types.attrsOf (types.submodule {
      options = {
        enable = mkOption {
          type = types.bool;
          default = true;
          description = "Whether to enable this VM";
        };

        hypervisor = mkOption {
          type = types.enum [ "cloud-hypervisor" "firecracker" "qemu" ];
          default = "cloud-hypervisor";
          description = "Hypervisor backend to use";
        };

        vcpus = mkOption {
          type = types.int;
          default = 1;
          description = "Number of virtual CPUs";
        };

        memory = mkOption {
          type = types.int;
          default = 512;
          description = "Memory in MB";
        };

        kernel = mkOption {
          type = types.nullOr types.package;
          default = null;
          description = "Custom kernel package to use";
        };

        kernelParams = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Additional kernel command line parameters";
        };

        initrdPath = mkOption {
          type = types.nullOr types.path;
          default = null;
          description = "Path to custom initrd";
        };

        networks = mkOption {
          type = types.listOf networkType;
          default = [ ];
          description = "Network interfaces";
        };

        volumes = mkOption {
          type = types.listOf volumeType;
          default = [ ];
          description = "Storage volumes";
        };

        nixosModules = mkOption {
          type = types.listOf types.deferredModule;
          default = [ ];
          description = "NixOS configuration modules for the VM";
        };

        graphics = mkOption {
          type = types.bool;
          default = false;
          description = "Enable graphical output";
        };

        autostart = mkOption {
          type = types.bool;
          default = false;
          description = "Automatically start the VM";
        };

        extraOptions = mkOption {
          type = types.attrs;
          default = { };
          description = "Extra hypervisor-specific options";
        };
      };
    });
    default = { };
    description = "Blixard VM definitions";
  };
}
