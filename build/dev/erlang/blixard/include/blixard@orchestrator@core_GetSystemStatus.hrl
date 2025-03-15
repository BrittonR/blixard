-record(get_system_status, {
    reply_with :: gleam@erlang@process:subject({ok,
            blixard@orchestrator@core:system_status()} |
        {error, binary()})
}).
