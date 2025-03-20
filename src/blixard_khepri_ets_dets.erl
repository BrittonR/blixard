-module(blixard_khepri_ets_dets).
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

% Use ETS for local storage during transition and DETS for persistence
-define(VM_TABLE, blixard_vms).
-define(HOST_TABLE, blixard_hosts).
-define(VM_DETS_FILE, blixard_vms).
-define(HOST_DETS_FILE, blixard_hosts).

% Start a simplified local storage system with persistence
start(Nodes, ClusterName) ->
    io:format("REAL: Starting local storage with name: ~p~n", [ClusterName]),
    
    % Get string version of cluster name for reference
    ClusterNameStr = case is_binary(ClusterName) of
        true -> binary_to_list(ClusterName);
        false -> ClusterName
    end,
    
    % Initialize storage with persistence
    case initialize_storage(ClusterNameStr) of
        ok ->
            io:format("REAL: Local storage initialized successfully~n"),
            {ok, {local_storage, ClusterNameStr}};
        {error, Reason} ->
            io:format("REAL: Failed to initialize local storage: ~p~n", [Reason]),
            {error, {connection_error, list_to_binary(io_lib:format("Failed to initialize storage: ~p", [Reason]))}}
    end.

% Initialize storage with persistence
initialize_storage(ClusterNameStr) ->
    try
        % Ensure data directory exists
        DataDir = "blixard_data/" ++ ClusterNameStr,
        ok = filelib:ensure_dir(DataDir ++ "/"),
        
        % VM table setup (ETS and DETS)
        VMDetsFile = DataDir ++ "/" ++ atom_to_list(?VM_DETS_FILE),
        case dets:open_file(?VM_DETS_FILE, [{file, VMDetsFile}, {type, set}]) of
            {ok, _} -> ok;
            {error, DetsReason} -> 
                io:format("REAL: Warning - Could not open DETS file for VMs: ~p~n", [DetsReason])
        end,
        
        case ets:info(?VM_TABLE) of
            undefined ->
                ets:new(?VM_TABLE, [named_table, set, public, {keypos, 1}]);
            _ ->
                ok
        end,
        
        % Load data from DETS to ETS if available
        dets:to_ets(?VM_DETS_FILE, ?VM_TABLE),
        
        % Host table setup (ETS and DETS)
        HostDetsFile = DataDir ++ "/" ++ atom_to_list(?HOST_DETS_FILE),
        case dets:open_file(?HOST_DETS_FILE, [{file, HostDetsFile}, {type, set}]) of
            {ok, _} -> ok;
            {error, HostDetsReason} -> 
                io:format("REAL: Warning - Could not open DETS file for Hosts: ~p~n", [HostDetsReason])
        end,
        
        case ets:info(?HOST_TABLE) of
            undefined ->
                ets:new(?HOST_TABLE, [named_table, set, public, {keypos, 1}]);
            _ ->
                ok
        end,
        
        % Load data from DETS to ETS if available
        dets:to_ets(?HOST_DETS_FILE, ?HOST_TABLE),
        
        % Start a background process for periodic persistence
        spawn(fun() -> persistence_loop(5000) end),
        
        ok
    catch
        InitError:InitReason:InitStackTrace ->
            io:format("REAL: Storage initialization failed: ~p:~p~n~p~n", [InitError, InitReason, InitStackTrace]),
            {error, InitReason}
    end.

% Background process to periodically save ETS to DETS
persistence_loop(Interval) ->
    timer:sleep(Interval),
    try
        % Persist VM table
        ets:to_dets(?VM_TABLE, ?VM_DETS_FILE),
        % Persist Host table
        ets:to_dets(?HOST_TABLE, ?HOST_DETS_FILE),
        % Sync DETS files to disk
        dets:sync(?VM_DETS_FILE),
        dets:sync(?HOST_DETS_FILE),
        io:format("REAL: Persisted data to disk successfully~n")
    catch
        LoopError:LoopReason ->
            io:format("REAL: Error persisting data to disk: ~p:~p~n", [LoopError, LoopReason])
    end,
    persistence_loop(Interval).

% Stop the storage system
stop(_Store) ->
    io:format("REAL: Stopping local storage~n"),
    
    % Ensure data is persisted before stopping
    try
        ets:to_dets(?VM_TABLE, ?VM_DETS_FILE),
        ets:to_dets(?HOST_TABLE, ?HOST_DETS_FILE),
        dets:sync(?VM_DETS_FILE),
        dets:sync(?HOST_DETS_FILE),
        dets:close(?VM_DETS_FILE),
        dets:close(?HOST_DETS_FILE),
        io:format("REAL: Storage data persisted and closed successfully~n")
    catch
        StopError:StopReason ->
            io:format("REAL: Error closing storage: ~p:~p~n", [StopError, StopReason])
    end,
    
    {ok, nil}.

% Convert binary ID to string if needed
ensure_string(Id) when is_binary(Id) ->
    binary_to_list(Id);
ensure_string(Id) ->
    Id.

% Put a VM into the store with automatic persistence
put_vm(_Store, VM) ->
    try
        Id = element(2, VM),
        IdStr = ensure_string(Id),
        io:format("REAL: Storing VM with ID: ~p~n", [Id]),
        
        % Store VM in ETS
        ets:insert(?VM_TABLE, {IdStr, VM}),
        
        % Immediate persistence for critical operations
        ets:to_dets(?VM_TABLE, ?VM_DETS_FILE),
        dets:sync(?VM_DETS_FILE),
        
        {ok, nil}
    catch
        PutVmError:PutVmReason ->
            io:format("REAL: Failed to store VM: ~p:~p~n", [PutVmError, PutVmReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to store VM: ~p", [PutVmReason]))}}
    end.

% Get a VM from the store
get_vm(_Store, Id) ->
    IdStr = ensure_string(Id),
    io:format("REAL: Getting VM with ID: ~p~n", [Id]),
    
    case ets:lookup(?VM_TABLE, IdStr) of
        [{_, VM}] ->
            {ok, VM};
        [] ->
            {error, not_found};
        Error ->
            io:format("REAL: Error retrieving VM: ~p~n", [Error]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to get VM: ~p", [Error]))}}
    end.

% List all VMs
list_vms(_Store) ->
    io:format("REAL: Listing all VMs~n"),
    try
        VMs = ets:tab2list(?VM_TABLE),
        {ok, [VM || {_, VM} <- VMs]}
    catch
        ListVmError:ListVmReason ->
            io:format("REAL: Failed to list VMs: ~p:~p~n", [ListVmError, ListVmReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to list VMs: ~p", [ListVmReason]))}}
    end.

% Delete a VM with automatic persistence
delete_vm(_Store, Id) ->
    IdStr = ensure_string(Id),
    io:format("REAL: Deleting VM with ID: ~p~n", [Id]),
    
    try
        % Delete from ETS
        ets:delete(?VM_TABLE, IdStr),
        
        % Immediate persistence for critical operations
        ets:to_dets(?VM_TABLE, ?VM_DETS_FILE),
        dets:sync(?VM_DETS_FILE),
        
        {ok, nil}
    catch
        DelVmError:DelVmReason ->
            io:format("REAL: Failed to delete VM: ~p:~p~n", [DelVmError, DelVmReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to delete VM: ~p", [DelVmReason]))}}
    end.

% Put a Host into the store with automatic persistence
put_host(_Store, Host) ->
    try
        Id = element(2, Host),
        IdStr = ensure_string(Id),
        io:format("REAL: Storing Host with ID: ~p~n", [Id]),
        
        % Store Host in ETS
        ets:insert(?HOST_TABLE, {IdStr, Host}),
        
        % Immediate persistence for critical operations
        ets:to_dets(?HOST_TABLE, ?HOST_DETS_FILE),
        dets:sync(?HOST_DETS_FILE),
        
        {ok, nil}
    catch
        PutHostError:PutHostReason ->
            io:format("REAL: Failed to store Host: ~p:~p~n", [PutHostError, PutHostReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to store Host: ~p", [PutHostReason]))}}
    end.

% Get a Host from the store
get_host(_Store, Id) ->
    IdStr = ensure_string(Id),
    io:format("REAL: Getting Host with ID: ~p~n", [Id]),
    
    case ets:lookup(?HOST_TABLE, IdStr) of
        [{_, Host}] ->
            {ok, Host};
        [] ->
            {error, not_found};
        Error ->
            io:format("REAL: Error retrieving Host: ~p~n", [Error]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to get Host: ~p", [Error]))}}
    end.

% List all Hosts
list_hosts(_Store) ->
    io:format("REAL: Listing all Hosts~n"),
    try
        Hosts = ets:tab2list(?HOST_TABLE),
        {ok, [Host || {_, Host} <- Hosts]}
    catch
        ListHostError:ListHostReason ->
            io:format("REAL: Failed to list Hosts: ~p:~p~n", [ListHostError, ListHostReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to list Hosts: ~p", [ListHostReason]))}}
    end.

% Delete a Host with automatic persistence
delete_host(_Store, Id) ->
    IdStr = ensure_string(Id),
    io:format("REAL: Deleting Host with ID: ~p~n", [Id]),
    
    try
        % Delete from ETS
        ets:delete(?HOST_TABLE, IdStr),
        
        % Immediate persistence for critical operations
        ets:to_dets(?HOST_TABLE, ?HOST_DETS_FILE),
        dets:sync(?HOST_DETS_FILE),
        
        {ok, nil}
    catch
        DelHostError:DelHostReason ->
            io:format("REAL: Failed to delete Host: ~p:~p~n", [DelHostError, DelHostReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to delete Host: ~p", [DelHostReason]))}}
    end.

% Update VM state with automatic persistence
update_vm_state(_Store, Id, State) ->
    IdStr = ensure_string(Id),
    io:format("REAL: Updating VM state for ID ~p to ~p~n", [Id, State]),
    
    try
        case ets:lookup(?VM_TABLE, IdStr) of
            [{_, VM}] ->
                % Create a new VM with updated state
                % The state is at position 7 in the MicroVm tuple
                NewVM = setelement(7, VM, State),
                ets:insert(?VM_TABLE, {IdStr, NewVM}),
                
                % Immediate persistence for critical operations
                ets:to_dets(?VM_TABLE, ?VM_DETS_FILE),
                dets:sync(?VM_DETS_FILE),
                
                {ok, nil};
            [] ->
                {error, not_found};
            LookupError ->
                io:format("REAL: Error retrieving VM for state update: ~p~n", [LookupError]),
                {error, {storage_error, list_to_binary(io_lib:format("Failed to update VM state: ~p", [LookupError]))}}
        end
    catch
        UpdateError:UpdateReason ->
            io:format("REAL: Failed to update VM state: ~p:~p~n", [UpdateError, UpdateReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to update VM state: ~p", [UpdateReason]))}}
    end.

% Assign VM to Host with automatic persistence
assign_vm_to_host(_Store, VmId, HostId) ->
    VmIdStr = ensure_string(VmId),
    HostIdStr = ensure_string(HostId),
    io:format("REAL: Assigning VM ~p to Host ~p~n", [VmId, HostId]),
    
    try
        % Update VM first
        case ets:lookup(?VM_TABLE, VmIdStr) of
            [{_, VM}] ->
                % Host ID is at position 8 in the VM tuple
                % We're assuming Some(HostId) here since we use option.Some in Gleam
                NewVM = setelement(8, VM, {some, HostId}),
                ets:insert(?VM_TABLE, {VmIdStr, NewVM});
            [] ->
                throw({vm_not_found, VmId})
        end,
        
        % Then update Host
        case ets:lookup(?HOST_TABLE, HostIdStr) of
            [{_, Host}] ->
                % VM IDs list is at position 9 in the Host tuple
                CurrentVmIds = element(9, Host),
                NewVmIds = [VmId | CurrentVmIds],
                NewHost = setelement(9, Host, NewVmIds),
                ets:insert(?HOST_TABLE, {HostIdStr, NewHost});
            [] ->
                throw({host_not_found, HostId})
        end,
        
        % Persist both changes
        ets:to_dets(?VM_TABLE, ?VM_DETS_FILE),
        ets:to_dets(?HOST_TABLE, ?HOST_DETS_FILE),
        dets:sync(?VM_DETS_FILE),
        dets:sync(?HOST_DETS_FILE),
        
        {ok, nil}
    catch
        throw:{vm_not_found, _} ->
            {error, not_found};
        throw:{host_not_found, _} ->
            {error, not_found};
        AssignError:AssignReason ->
            io:format("REAL: Failed to assign VM to Host: ~p:~p~n", [AssignError, AssignReason]),
            {error, {storage_error, list_to_binary(io_lib:format("Failed to assign VM to Host: ~p", [AssignReason]))}}
    end.
