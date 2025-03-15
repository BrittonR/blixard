-record(state, {
    host_id :: binary(),
    store :: blixard@storage@khepri_store:khepri(),
    vms :: gleam@dict:dict(binary(), blixard@host_agent@agent:vm_process()),
    resources :: blixard@domain@types:resources(),
    schedulable :: boolean()
}).
