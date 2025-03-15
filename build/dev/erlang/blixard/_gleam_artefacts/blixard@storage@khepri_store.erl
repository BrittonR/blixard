-module(blixard@storage@khepri_store).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([start/2, stop/1, put_vm/2, get_vm/2, list_vms/1, delete_vm/2, put_host/2, get_host/2, list_hosts/1, delete_host/2, update_vm_state/3, assign_vm_to_host/3]).
-export_type([khepri/0, khepri_error/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/storage/khepri_store.gleam\n").

-opaque khepri() :: {khepri, binary()}.

-type khepri_error() :: {connection_error, binary()} |
    {consensus_error, binary()} |
    {storage_error, binary()} |
    not_found |
    {invalid_data, binary()}.

-file("src/blixard/storage/khepri_store.gleam", 40).
?DOC(" Connect to Khepri and start the store\n").
-spec start(list(binary()), binary()) -> {ok, khepri()} |
    {error, khepri_error()}.
start(Nodes, Cluster_name) ->
    blixard_khepri_store:start(Nodes, Cluster_name).

-file("src/blixard/storage/khepri_store.gleam", 47).
?DOC(" Stop the Khepri store\n").
-spec stop(khepri()) -> {ok, nil} | {error, khepri_error()}.
stop(Store) ->
    blixard_khepri_store:stop(Store).

-file("src/blixard/storage/khepri_store.gleam", 51).
?DOC(" Store a MicroVM in Khepri\n").
-spec put_vm(khepri(), blixard@domain@types:micro_vm()) -> {ok, nil} |
    {error, khepri_error()}.
put_vm(Store, Vm) ->
    blixard_khepri_store:put_vm(Store, Vm).

-file("src/blixard/storage/khepri_store.gleam", 55).
?DOC(" Retrieve a MicroVM from Khepri\n").
-spec get_vm(khepri(), binary()) -> {ok, blixard@domain@types:micro_vm()} |
    {error, khepri_error()}.
get_vm(Store, Id) ->
    blixard_khepri_store:get_vm(Store, Id).

-file("src/blixard/storage/khepri_store.gleam", 59).
?DOC(" List all MicroVMs\n").
-spec list_vms(khepri()) -> {ok, list(blixard@domain@types:micro_vm())} |
    {error, khepri_error()}.
list_vms(Store) ->
    blixard_khepri_store:list_vms(Store).

-file("src/blixard/storage/khepri_store.gleam", 63).
?DOC(" Delete a MicroVM\n").
-spec delete_vm(khepri(), binary()) -> {ok, nil} | {error, khepri_error()}.
delete_vm(Store, Id) ->
    blixard_khepri_store:delete_vm(Store, Id).

-file("src/blixard/storage/khepri_store.gleam", 67).
?DOC(" Store a Host in Khepri\n").
-spec put_host(khepri(), blixard@domain@types:host()) -> {ok, nil} |
    {error, khepri_error()}.
put_host(Store, Host) ->
    blixard_khepri_store:put_host(Store, Host).

-file("src/blixard/storage/khepri_store.gleam", 71).
?DOC(" Retrieve a Host from Khepri\n").
-spec get_host(khepri(), binary()) -> {ok, blixard@domain@types:host()} |
    {error, khepri_error()}.
get_host(Store, Id) ->
    blixard_khepri_store:get_host(Store, Id).

-file("src/blixard/storage/khepri_store.gleam", 75).
?DOC(" List all Hosts\n").
-spec list_hosts(khepri()) -> {ok, list(blixard@domain@types:host())} |
    {error, khepri_error()}.
list_hosts(Store) ->
    blixard_khepri_store:list_hosts(Store).

-file("src/blixard/storage/khepri_store.gleam", 79).
?DOC(" Delete a Host\n").
-spec delete_host(khepri(), binary()) -> {ok, nil} | {error, khepri_error()}.
delete_host(Store, Id) ->
    blixard_khepri_store:delete_host(Store, Id).

-file("src/blixard/storage/khepri_store.gleam", 83).
?DOC(" Update the state of a VM\n").
-spec update_vm_state(khepri(), binary(), blixard@domain@types:resource_state()) -> {ok,
        nil} |
    {error, khepri_error()}.
update_vm_state(Store, Id, State) ->
    blixard_khepri_store:update_vm_state(Store, Id, State).

-file("src/blixard/storage/khepri_store.gleam", 91).
?DOC(" Update host assignment for a VM\n").
-spec assign_vm_to_host(khepri(), binary(), binary()) -> {ok, nil} |
    {error, khepri_error()}.
assign_vm_to_host(Store, Vm_id, Host_id) ->
    blixard_khepri_store:assign_vm_to_host(Store, Vm_id, Host_id).
