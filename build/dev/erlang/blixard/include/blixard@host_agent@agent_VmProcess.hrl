-record(vm_process, {
    vm_id :: binary(),
    pid :: gleam@option:option(gleam@erlang@process:pid_()),
    state :: blixard@domain@types:resource_state()
}).
