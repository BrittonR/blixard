-record(host, {
    id :: binary(),
    name :: binary(),
    description :: gleam@option:option(binary()),
    control_ip :: binary(),
    connected :: boolean(),
    available_resources :: blixard@domain@types:resources(),
    total_resources :: blixard@domain@types:resources(),
    vm_ids :: list(binary()),
    schedulable :: boolean(),
    tags :: list(binary()),
    labels :: gleam@dict:dict(binary(), binary()),
    created_at :: binary(),
    updated_at :: binary()
}).
