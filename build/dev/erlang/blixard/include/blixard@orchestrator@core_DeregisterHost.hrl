-record(deregister_host, {
    host_id :: binary(),
    reply_with :: gleam@erlang@process:subject({ok, nil} | {error, binary()})
}).
