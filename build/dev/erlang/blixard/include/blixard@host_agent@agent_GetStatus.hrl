-record(get_status, {
    reply_with :: gleam@erlang@process:subject({ok,
            blixard@host_agent@agent:host_status()} |
        {error, binary()})
}).
