-record(update_resources, {
    resources :: blixard@domain@types:resources(),
    reply_with :: gleam@erlang@process:subject({ok, nil} | {error, binary()})
}).
