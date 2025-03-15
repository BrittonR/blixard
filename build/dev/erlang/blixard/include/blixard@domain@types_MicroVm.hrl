-record(micro_vm, {
    id :: binary(),
    name :: binary(),
    description :: gleam@option:option(binary()),
    vm_type :: blixard@domain@types:vm_type(),
    resources :: blixard@domain@types:resources(),
    state :: blixard@domain@types:resource_state(),
    host_id :: gleam@option:option(binary()),
    storage_volumes :: list(blixard@domain@types:storage_volume()),
    network_interfaces :: list(blixard@domain@types:network_interface()),
    tailscale_config :: blixard@domain@types:tailscale_config(),
    nixos_config :: blixard@domain@types:nixos_config(),
    labels :: gleam@dict:dict(binary(), binary()),
    created_at :: binary(),
    updated_at :: binary()
}).
