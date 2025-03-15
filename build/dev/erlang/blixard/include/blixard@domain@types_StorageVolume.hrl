-record(storage_volume, {
    id :: binary(),
    name :: binary(),
    size_gb :: integer(),
    path :: binary(),
    persistent :: boolean()
}).
