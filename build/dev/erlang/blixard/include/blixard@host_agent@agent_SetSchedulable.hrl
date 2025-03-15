-record(set_schedulable, {
    schedulable :: boolean(),
    reply_with :: gleam@erlang@process:subject({ok, nil} | {error, binary()})
}).
