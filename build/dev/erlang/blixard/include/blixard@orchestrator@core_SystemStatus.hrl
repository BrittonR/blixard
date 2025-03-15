-record(system_status, {
    vm_count :: integer(),
    host_count :: integer(),
    vms_by_state :: gleam@dict:dict(blixard@domain@types:resource_state(), integer())
}).
