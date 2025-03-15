-record(state, {
    store :: blixard@storage@khepri_store:khepri(),
    hosts :: gleam@dict:dict(binary(), gleam@erlang@process:subject(blixard@host_agent@agent:command())),
    scheduling_strategy :: blixard@scheduler@scheduler:scheduling_strategy()
}).
