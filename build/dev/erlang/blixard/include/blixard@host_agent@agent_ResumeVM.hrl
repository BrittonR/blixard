-record(resume_vm, {
    vm_id :: binary(),
    reply_with :: gleam@erlang@process:subject({ok, nil} | {error, binary()})
}).
