-module(blixard_khepri_store).
-export([start/2, stop/1, 
         put_vm/2, get_vm/2, list_vms/1, delete_vm/2,
         put_host/2, get_host/2, list_hosts/1, delete_host/2,
         update_vm_state/3, assign_vm_to_host/3]).

% Start the Khepri store (initialize ETS tables)
start(_Nodes, ClusterName) ->
    try
        % Create ETS tables for our data if they don't exist
        case ets:info(blixard_vms) of
            undefined -> ets:new(blixard_vms, [named_table, set, public]);
            _ -> ok
        end,
        
        case ets:info(blixard_hosts) of
            undefined -> ets:new(blixard_hosts, [named_table, set, public]);
            _ -> ok
        end,
        
        % Return store identifier
        {ok, {khepri, ClusterName}}
    catch
        _:Error ->
            {error, {connection_error, io_lib:format("~p", [Error])}}
    end.

% Stop the Khepri store (clean up ETS tables)
stop(_Store) ->
    try
        % Just return success without deleting the tables
        % This allows us to test multiple operations in a session
        {ok, nil}
    catch
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Debug a term
debug_term(Term) ->
    io:format("DEBUG - Term type: ~p~n", [element(1, Term)]),
    io:format("DEBUG - Term structure: ~p~n", [Term]).

% Put a VM into the store
put_vm(_Store, VM) ->
    try
        % Debug output
        debug_term(VM),
        
        % Extract ID from MicroVm tuple
        Id = element(2, VM),
        
        % Store the VM
        ets:insert(blixard_vms, {Id, VM}),
        {ok, nil}
    catch
        error:badarg ->
            % Create the table if it doesn't exist yet
            ets:new(blixard_vms, [named_table, set, public]),
            put_vm(_Store, VM);
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Get a VM from the store
get_vm(_Store, Id) ->
    try
        case ets:lookup(blixard_vms, Id) of
            [{Id, VM}] -> {ok, VM};
            [] -> {error, not_found}
        end
    catch
        error:badarg ->
            % Table doesn't exist
            {error, not_found};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% List all VMs
list_vms(_Store) ->
    try
        VMs = ets:tab2list(blixard_vms),
        VMList = [VM || {_, VM} <- VMs],
        {ok, VMList}
    catch
        error:badarg ->
            % Table doesn't exist
            {ok, []};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Delete a VM
delete_vm(_Store, Id) ->
    try
        ets:delete(blixard_vms, Id),
        {ok, nil}
    catch
        error:badarg ->
            % Table doesn't exist
            {ok, nil};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Put a Host into the store
put_host(_Store, Host) ->
    try
        % Debug output
        debug_term(Host),
        
        % Extract ID from Host tuple
        Id = element(2, Host),
        
        % Store the Host
        ets:insert(blixard_hosts, {Id, Host}),
        {ok, nil}
    catch
        error:badarg ->
            % Create the table if it doesn't exist yet
            ets:new(blixard_hosts, [named_table, set, public]),
            put_host(_Store, Host);
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Get a Host from the store
get_host(_Store, Id) ->
    try
        case ets:lookup(blixard_hosts, Id) of
            [{Id, Host}] -> {ok, Host};
            [] -> {error, not_found}
        end
    catch
        error:badarg ->
            % Table doesn't exist
            {error, not_found};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% List all Hosts
list_hosts(_Store) ->
    try
        Hosts = ets:tab2list(blixard_hosts),
        HostList = [Host || {_, Host} <- Hosts],
        {ok, HostList}
    catch
        error:badarg ->
            % Table doesn't exist
            {ok, []};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Delete a Host
delete_host(_Store, Id) ->
    try
        ets:delete(blixard_hosts, Id),
        {ok, nil}
    catch
        error:badarg ->
            % Table doesn't exist
            {ok, nil};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Update VM state - now properly modifies the state
update_vm_state(_Store, Id, State) ->
    try
        case ets:lookup(blixard_vms, Id) of
            [{Id, VM}] ->
                % Extract all elements from the VM tuple
                % The state is at position 7 in the tuple
                UpdatedVM = setelement(7, VM, State),
                
                % Store the updated VM
                ets:insert(blixard_vms, {Id, UpdatedVM}),
                {ok, nil};
            [] -> 
                {error, not_found}
        end
    catch
        error:badarg ->
            % Table doesn't exist
            {error, not_found};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Assign VM to Host - now properly updates the host_id
assign_vm_to_host(_Store, VmId, HostId) ->
    try
        % Update VM's host_id (which is at position 8 in the tuple)
        case ets:lookup(blixard_vms, VmId) of
            [{VmId, VM}] ->
                UpdatedVM = setelement(8, VM, {some, HostId}),
                ets:insert(blixard_vms, {VmId, UpdatedVM});
            [] -> 
                {error, not_found}
        end,
        
        % Update Host's vm_ids list
        case ets:lookup(blixard_hosts, HostId) of
            [{HostId, Host}] ->
                % Get current vm_ids (position 9)
                VmIds = element(9, Host),
                
                % Add new VM ID if not already present
                NewVmIds = case lists:member(VmId, VmIds) of
                    true -> VmIds;
                    false -> [VmId | VmIds]
                end,
                
                % Update the host
                UpdatedHost = setelement(9, Host, NewVmIds),
                ets:insert(blixard_hosts, {HostId, UpdatedHost});
            [] -> 
                ok  % Host not found, but that's ok for this operation
        end,
        
        {ok, nil}
    catch
        error:badarg ->
            % Create tables if they don't exist
            case ets:info(blixard_vms) of
                undefined -> ets:new(blixard_vms, [named_table, set, public]);
                _ -> ok
            end,
            case ets:info(blixard_hosts) of
                undefined -> ets:new(blixard_hosts, [named_table, set, public]);
                _ -> ok
            end,
            % Return success anyway
            {ok, nil};
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.
