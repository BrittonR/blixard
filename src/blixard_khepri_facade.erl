%% src/blixard_khepri_facade.erl
-module(blixard_khepri_facade).
-export([
    start/2, 
    stop/1, 
    put_vm/2, 
    get_vm/2, 
    list_vms/1, 
    delete_vm/2,
    put_host/2, 
    get_host/2, 
    list_hosts/1, 
    delete_host/2,
    update_vm_state/3, 
    assign_vm_to_host/3
]).

%% Determine storage implementation to use
get_storage_impl() ->
    case os:getenv("BLIXARD_STORAGE_MODE") of
        "ets_dets" -> ets_dets;
        "khepri" -> khepri;
        _ -> mock
    end.

%% Check if a specific operation should use the ETS/DETS implementation
use_ets_dets_for_operation(Operation) ->
    case os:getenv("BLIXARD_ETSDETS_OPS") of
        false -> false;
        [] -> false;
        Ops ->
            % Check if the operation is in the comma-separated list
            OpsList = string:tokens(Ops, ","),
            lists:member(atom_to_list(Operation), OpsList)
    end.

%% Check if a specific operation should use the Khepri implementation
use_khepri_for_operation(Operation) ->
    case os:getenv("BLIXARD_KHEPRI_OPS") of
        false -> false;
        [] -> false;
        Ops ->
            % Check if the operation is in the comma-separated list
            OpsList = string:tokens(Ops, ","),
            lists:member(atom_to_list(Operation), OpsList)
    end.

%% Choose the appropriate implementation module for an operation
choose_impl_for_operation(Operation) ->
    StorageMode = get_storage_impl(),
    case StorageMode of
        ets_dets ->
            case use_ets_dets_for_operation(Operation) of
                true -> blixard_khepri_ets_dets;
                false -> blixard_khepri_mock
            end;
        khepri ->
            case use_khepri_for_operation(Operation) of
                true -> blixard_khepri_khepri;
                false -> blixard_khepri_mock
            end;
        _ ->
            blixard_khepri_mock
    end.

%% Start function that delegates to the right implementation
start(Nodes, ClusterName) ->
    Impl = choose_impl_for_operation(start),
    io:format("[FACADE] Using ~p for 'start'~n", [Impl]),
    Impl:start(Nodes, ClusterName).

%% Stop function that delegates to the right implementation
stop(Store) ->
    Impl = choose_impl_for_operation(stop),
    io:format("[FACADE] Using ~p for 'stop'~n", [Impl]),
    Impl:stop(Store).

%% VM operations
put_vm(Store, VM) ->
    Impl = choose_impl_for_operation(put_vm),
    io:format("[FACADE] Using ~p for 'put_vm'~n", [Impl]),
    Impl:put_vm(Store, VM).

get_vm(Store, Id) ->
    Impl = choose_impl_for_operation(get_vm),
    io:format("[FACADE] Using ~p for 'get_vm'~n", [Impl]),
    Impl:get_vm(Store, Id).

list_vms(Store) ->
    Impl = choose_impl_for_operation(list_vms),
    io:format("[FACADE] Using ~p for 'list_vms'~n", [Impl]),
    Impl:list_vms(Store).

delete_vm(Store, Id) ->
    Impl = choose_impl_for_operation(delete_vm),
    io:format("[FACADE] Using ~p for 'delete_vm'~n", [Impl]),
    Impl:delete_vm(Store, Id).

%% Host operations
put_host(Store, Host) ->
    Impl = choose_impl_for_operation(put_host),
    io:format("[FACADE] Using ~p for 'put_host'~n", [Impl]),
    Impl:put_host(Store, Host).

get_host(Store, Id) ->
    Impl = choose_impl_for_operation(get_host),
    io:format("[FACADE] Using ~p for 'get_host'~n", [Impl]),
    Impl:get_host(Store, Id).

list_hosts(Store) ->
    Impl = choose_impl_for_operation(list_hosts),
    io:format("[FACADE] Using ~p for 'list_hosts'~n", [Impl]),
    Impl:list_hosts(Store).

delete_host(Store, Id) ->
    Impl = choose_impl_for_operation(delete_host),
    io:format("[FACADE] Using ~p for 'delete_host'~n", [Impl]),
    Impl:delete_host(Store, Id).

%% State and relationship operations
update_vm_state(Store, Id, State) ->
    Impl = choose_impl_for_operation(update_vm_state),
    io:format("[FACADE] Using ~p for 'update_vm_state'~n", [Impl]),
    Impl:update_vm_state(Store, Id, State).

assign_vm_to_host(Store, VmId, HostId) ->
    Impl = choose_impl_for_operation(assign_vm_to_host),
    io:format("[FACADE] Using ~p for 'assign_vm_to_host'~n", [Impl]),
    Impl:assign_vm_to_host(Store, VmId, HostId).
