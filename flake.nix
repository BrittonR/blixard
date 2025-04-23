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
          packages = with pkgs; [ gleam erlang rebar3 elixir jq];
          
          # Add environment variables for Khepri configuration
          shellHook = ''
  # export BLIXARD_STORAGE_MODE="ets_dets"
  # export BLIXARD_ETSDETS_OPS="start,stop,get_vm,get_host,list_vms,list_hosts"
  export BLIXARD_STORAGE_MODE="khepri"

export BLIXARD_KHEPRI_OPS="start,stop,put_vm,get_vm,list_vms,delete_vm,put_host,get_host,list_hosts,delete_host,update_vm_state,assign_vm_to_host"

  echo "BLIXARD_STORAGE_MODE set to: $BLIXARD_STORAGE_MODE"
  echo "BLIXARD_ETSDETS_OPS set to: $BLIXARD_ETSDETS_OPS"
'';
        };
      });
    };
}
