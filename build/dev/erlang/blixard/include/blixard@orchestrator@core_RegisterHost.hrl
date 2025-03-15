-record(register_host, {
    host :: blixard@domain@types:host(),
    reply_with :: gleam@erlang@process:subject({ok, blixard@domain@types:host()} |
        {error, binary()})
}).
