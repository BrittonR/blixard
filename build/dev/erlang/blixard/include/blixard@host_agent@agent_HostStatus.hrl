-record(host_status, {
    host_id :: binary(),
    vm_count :: integer(),
    running_vms :: list(binary()),
    available_resources :: blixard@domain@types:resources(),
    schedulable :: boolean()
}).
