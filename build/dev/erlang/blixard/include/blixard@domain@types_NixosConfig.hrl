-record(nixos_config, {
    config_path :: binary(),
    overrides :: gleam@dict:dict(binary(), binary()),
    cache_url :: gleam@option:option(binary())
}).
