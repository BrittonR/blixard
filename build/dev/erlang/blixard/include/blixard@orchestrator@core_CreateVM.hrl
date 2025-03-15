-record(create_vm, {
    vm :: blixard@domain@types:micro_vm(),
    reply_with :: gleam@erlang@process:subject({ok,
            blixard@domain@types:micro_vm()} |
        {error, binary()})
}).
