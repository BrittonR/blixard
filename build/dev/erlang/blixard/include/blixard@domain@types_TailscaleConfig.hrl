-record(tailscale_config, {
    enabled :: boolean(),
    auth_key :: gleam@option:option(binary()),
    hostname :: binary(),
    tags :: list(binary()),
    direct_client :: boolean()
}).
