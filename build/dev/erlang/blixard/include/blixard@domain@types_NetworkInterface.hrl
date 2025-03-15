-record(network_interface, {
    id :: binary(),
    name :: binary(),
    ipv4_address :: gleam@option:option(binary()),
    ipv6_address :: gleam@option:option(binary()),
    mac_address :: gleam@option:option(binary())
}).
