-module(blixard_khepri_store).
-export([start/2, stop/1, 
         put_vm/2, get_vm/2, list_vms/1, delete_vm/2,
         put_host/2, get_host/2, list_hosts/1, delete_host/2,
         update_vm_state/3, assign_vm_to_host/3]).

% Start the Khepri store (initialize ETS tables)
start(_Nodes, ClusterName) ->
    try
        io:format("~n[KHEPRI DEBUG] Initializing Khepri store with cluster name: ~p~n", [ClusterName]),
        
        % Create ETS tables for our data if they don't exist
        case ets:info(blixard_vms) of
            undefined -> 
                io:format("[KHEPRI DEBUG] Creating blixard_vms table~n"),
                ets:new(blixard_vms, [named_table, set, public]);
            _ -> 
                io:format("[KHEPRI DEBUG] Using existing blixard_vms table~n"),
                ok
        end,
        
        case ets:info(blixard_hosts) of
            undefined -> 
                io:format("[KHEPRI DEBUG] Creating blixard_hosts table~n"),
                ets:new(blixard_hosts, [named_table, set, public]);
            _ -> 
                io:format("[KHEPRI DEBUG] Using existing blixard_hosts table~n"),
                ok
        end,
        
        % Return store identifier
        io:format("[KHEPRI DEBUG] Store initialized successfully~n"),
        {ok, {khepri, ClusterName}}
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Failed to start store: ~p~n", [Error]),
            {error, {connection_error, io_lib:format("~p", [Error])}}
    end.

% Stop the Khepri store (clean up ETS tables)
stop(_Store) ->
    try
        io:format("~n[KHEPRI DEBUG] Stopping Khepri store~n"),
        % Just return success without deleting the tables
        % This allows us to test multiple operations in a session
        {ok, nil}
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Failed to stop store: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Debug a term
debug_term(Term) ->
    io:format("[KHEPRI DEBUG] Term type: ~p~n", [element(1, Term)]),
    io:format("[KHEPRI DEBUG] Term structure: ~p~n", [Term]).

% Put a VM into the store
put_vm(_Store, VM) ->
    try
        % Enhanced debug output
        io:format("~n[KHEPRI DEBUG] Storing VM in Khepri:~n"),
        io:format("  - VM ID: ~p~n", [element(2, VM)]),
        io:format("  - VM Name: ~p~n", [element(3, VM)]),
        io:format("  - VM State: ~p~n", [element(7, VM)]),
        io:format("  - VM Host ID: ~p~n", [element(8, VM)]),
        
        % Extract ID from MicroVm tuple
        Id = element(2, VM),
        
        % Store the VM
        ets:insert(blixard_vms, {Id, VM}),
        io:format("[KHEPRI DEBUG] VM ~p stored successfully~n", [Id]),
        {ok, nil}
    catch
        error:badarg ->
            % Create the table if it doesn't exist yet
            io:format("[KHEPRI DEBUG] Creating missing blixard_vms table~n"),
            ets:new(blixard_vms, [named_table, set, public]),
            put_vm(_Store, VM);
        _:Error ->
            io:format("[KHEPRI ERROR] Failed to store VM: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Get a VM from the store
get_vm(_Store, Id) ->
    try
        io:format("~n[KHEPRI DEBUG] Getting VM with ID: ~p~n", [Id]),
        case ets:lookup(blixard_vms, Id) of
            [{Id, VM}] -> 
                io:format("[KHEPRI DEBUG] VM found: ~p~n", [element(3, VM)]),
                {ok, VM};
            [] -> 
                io:format("[KHEPRI DEBUG] VM with ID ~p not found~n", [Id]),
                {error, not_found}
        end
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] VM table doesn't exist~n"),
            {error, not_found};
        _:Error ->
            io:format("[KHEPRI ERROR] Error getting VM: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% List all VMs
list_vms(_Store) ->
    try
        io:format("~n[KHEPRI DEBUG] Listing all VMs~n"),
        VMs = ets:tab2list(blixard_vms),
        VMList = [VM || {_, VM} <- VMs],
        io:format("[KHEPRI DEBUG] Found ~p VMs~n", [length(VMList)]),
        {ok, VMList}
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] VM table doesn't exist, returning empty list~n"),
            {ok, []};
        _:Error ->
            io:format("[KHEPRI ERROR] Error listing VMs: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Delete a VM
delete_vm(_Store, Id) ->
    try
        io:format("~n[KHEPRI DEBUG] Deleting VM with ID: ~p~n", [Id]),
        ets:delete(blixard_vms, Id),
        io:format("[KHEPRI DEBUG] VM deleted successfully~n"),
        {ok, nil}
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] VM table doesn't exist, nothing to delete~n"),
            {ok, nil};
        _:Error ->
            io:format("[KHEPRI ERROR] Error deleting VM: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Put a Host into the store
put_host(_Store, Host) ->
    try
        % Enhanced debug output
        io:format("~n[KHEPRI DEBUG] Storing Host in Khepri:~n"),
        io:format("  - Host ID: ~p~n", [element(2, Host)]),
        io:format("  - Host Name: ~p~n", [element(3, Host)]),
        io:format("  - Host IP: ~p~n", [element(5, Host)]),
        
        % Extract ID from Host tuple
        Id = element(2, Host),
        
        % Store the Host
        ets:insert(blixard_hosts, {Id, Host}),
        io:format("[KHEPRI DEBUG] Host ~p stored successfully~n", [Id]),
        {ok, nil}
    catch
        error:badarg ->
            % Create the table if it doesn't exist yet
            io:format("[KHEPRI DEBUG] Creating missing blixard_hosts table~n"),
            ets:new(blixard_hosts, [named_table, set, public]),
            put_host(_Store, Host);
        _:Error ->
            io:format("[KHEPRI ERROR] Failed to store Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Get a Host from the store
get_host(_Store, Id) ->
    try
        io:format("~n[KHEPRI DEBUG] Getting Host with ID: ~p~n", [Id]),
        case ets:lookup(blixard_hosts, Id) of
            [{Id, Host}] -> 
                io:format("[KHEPRI DEBUG] Host found: ~p~n", [element(3, Host)]),
                {ok, Host};
            [] -> 
                io:format("[KHEPRI DEBUG] Host with ID ~p not found~n", [Id]),
                {error, not_found}
        end
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] Host table doesn't exist~n"),
            {error, not_found};
        _:Error ->
            io:format("[KHEPRI ERROR] Error getting Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% List all Hosts
list_hosts(_Store) ->
    try
        io:format("~n[KHEPRI DEBUG] Listing all Hosts~n"),
        Hosts = ets:tab2list(blixard_hosts),
        HostList = [Host || {_, Host} <- Hosts],
        io:format("[KHEPRI DEBUG] Found ~p Hosts~n", [length(HostList)]),
        {ok, HostList}
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] Host table doesn't exist, returning empty list~n"),
            {ok, []};
        _:Error ->
            io:format("[KHEPRI ERROR] Error listing Hosts: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Delete a Host
delete_host(_Store, Id) ->
    try
        io:format("~n[KHEPRI DEBUG] Deleting Host with ID: ~p~n", [Id]),
        ets:delete(blixard_hosts, Id),
        io:format("[KHEPRI DEBUG] Host deleted successfully~n"),
        {ok, nil}
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] Host table doesn't exist, nothing to delete~n"),
            {ok, nil};
        _:Error ->
            io:format("[KHEPRI ERROR] Error deleting Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Update VM state - now with enhanced debugging
update_vm_state(_Store, Id, State) ->
    try
        io:format("~n[KHEPRI DEBUG] Updating VM state in Khepri:~n"),
        io:format("  - VM ID: ~p~n", [Id]),
        io:format("  - New State: ~p~n", [State]),
        
        case ets:lookup(blixard_vms, Id) of
            [{Id, VM}] ->
                % Extract all elements from the VM tuple
                OldState = element(7, VM),
                io:format("  - Old State: ~p~n", [OldState]),
                
                % The state is at position 7 in the tuple
                UpdatedVM = setelement(7, VM, State),
                
                % Store the updated VM
                ets:insert(blixard_vms, {Id, UpdatedVM}),
                io:format("[KHEPRI DEBUG] VM ~p state updated: ~p -> ~p~n", 
                          [Id, OldState, State]),
                {ok, nil};
            [] -> 
                io:format("[KHEPRI DEBUG] VM ~p not found for state update~n", [Id]),
                {error, not_found}
        end
    catch
        error:badarg ->
            % Table doesn't exist
            io:format("[KHEPRI DEBUG] VM table doesn't exist for state update~n"),
            {error, not_found};
        _:Error ->
            io:format("[KHEPRI ERROR] Error updating VM state: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Assign VM to Host - now with enhanced debugging
assign_vm_to_host(_Store, VmId, HostId) ->
    try
        io:format("~n[KHEPRI DEBUG] Assigning VM to Host:~n"),
        io:format("  - VM ID: ~p~n", [VmId]),
        io:format("  - Host ID: ~p~n", [HostId]),
        
        % Update VM's host_id (which is at position 8 in the tuple)
        case ets:lookup(blixard_vms, VmId) of
            [{VmId, VM}] ->
                OldHostId = element(8, VM),
                io:format("  - Old Host ID: ~p~n", [OldHostId]),
                
                UpdatedVM = setelement(8, VM, {some, HostId}),
                ets:insert(blixard_vms, {VmId, UpdatedVM}),
                io:format("[KHEPRI DEBUG] VM ~p host updated: ~p -> ~p~n", 
                          [VmId, OldHostId, {some, HostId}]);
            [] -> 
                io:format("[KHEPRI DEBUG] VM ~p not found for host assignment~n", [VmId]),
                {error, not_found}
        end,
        
        % Update Host's vm_ids list
        case ets:lookup(blixard_hosts, HostId) of
            [{HostId, Host}] ->
                % Get current vm_ids (position 9)
                VmIds = element(9, Host),
                io:format("  - Current VM IDs on host: ~p~n", [VmIds]),
                
                % Add new VM ID if not already present
                NewVmIds = case lists:member(VmId, VmIds) of
                    true -> 
                        io:format("  - VM already assigned to host~n"),
                        VmIds;
                    false -> 
                        io:format("  - Adding VM to host's VM list~n"),
                        [VmId | VmIds]
                end,
                
                % Update the host
                UpdatedHost = setelement(9, Host, NewVmIds),
                ets:insert(blixard_hosts, {HostId, UpdatedHost}),
                io:format("[KHEPRI DEBUG] Host ~p VM list updated: ~p~n", [HostId, NewVmIds]);
            [] -> 
                io:format("[KHEPRI DEBUG] Host ~p not found for VM assignment~n", [HostId])
        end,
        
        io:format("[KHEPRI DEBUG] VM to Host assignment completed successfully~n"),
        {ok, nil}
    catch
        error:badarg ->
            % Create tables if they don't exist
            io:format("[KHEPRI DEBUG] Tables don't exist for VM-Host assignment~n"),
            case ets:info(blixard_vms) of
                undefined -> 
                    io:format("[KHEPRI DEBUG] Creating missing blixard_vms table~n"),
                    ets:new(blixard_vms, [named_table, set, public]);
                _ -> ok
            end,
            case ets:info(blixard_hosts) of
                undefined -> 
                    io:format("[KHEPRI DEBUG] Creating missing blixard_hosts table~n"),
                    ets:new(blixard_hosts, [named_table, set, public]);
                _ -> ok
            end,
            % Return error
            io:format("[KHEPRI ERROR] Tables were missing for VM-Host assignment~n"),
            {error, {storage_error, "Tables were missing for assignment"}};
        _:Error ->
            io:format("[KHEPRI ERROR] Error assigning VM to Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.
