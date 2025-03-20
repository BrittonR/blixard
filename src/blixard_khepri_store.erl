-module(blixard_khepri_store).
-export([start/2, stop/1, 
         put_vm/2, get_vm/2, list_vms/1, delete_vm/2,
         put_host/2, get_host/2, list_hosts/1, delete_host/2,
         update_vm_state/3, assign_vm_to_host/3]).

% Root paths for different resource types
-define(VMS_PATH, [blixard, vms]).
-define(HOSTS_PATH, [blixard, hosts]).
-define(NETWORKS_PATH, [blixard, networks]).

% Start the Khepri store
start(Nodes, ClusterName) ->
    try
        io:format("~n[KHEPRI] Starting real Khepri store with cluster name: ~p~n", [ClusterName]),
        
        % Convert Gleam string to atom for cluster name
        ClusterAtom = binary_to_atom(ClusterName),
        
        % Define data directory
        DataDir = "khepri_data_" ++ binary_to_list(ClusterName),
        
        % Initialize Khepri app
        application:ensure_all_started(khepri),
        
        % Build Khepri store options
        StoreOptions = #{
            store_id => ClusterAtom,
            data_dir => DataDir
        },
        
        % Start local Khepri store
        case khepri:start(StoreOptions) of
            {ok, _} ->
                io:format("[KHEPRI] Khepri store started successfully~n"),
                {ok, {khepri, ClusterName}};
            {error, {already_started, _}} ->
                io:format("[KHEPRI] Khepri store already running~n"),
                {ok, {khepri, ClusterName}};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Failed to start Khepri: ~p~n", [Reason]),
                {error, {connection_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Unexpected error starting Khepri: ~p~n", [Error]),
            {error, {connection_error, io_lib:format("~p", [Error])}}
    end.

% Stop the Khepri store
stop(_Store) ->
    try
        io:format("~n[KHEPRI] Stopping Khepri store~n"),
        % Not actually stopping Khepri in this implementation to avoid disrupting ongoing work
        {ok, nil}
    catch
        _:Error ->
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Helper function to build a VM path
vm_path(Id) ->
    ?VMS_PATH ++ [Id].

% Helper function to build a host path
host_path(Id) ->
    ?HOSTS_PATH ++ [Id].

% Put a VM into the store
put_vm(_Store, VM) ->
    try
        % Extract ID from MicroVm tuple
        Id = element(2, VM),
        
        io:format("~n[KHEPRI] Storing VM in Khepri:~n"),
        io:format("  - VM ID: ~p~n", [Id]),
        
        % Store the VM using Khepri
        case khepri:put(vm_path(Id), VM) of
            ok ->
                io:format("[KHEPRI] VM ~p stored successfully~n", [Id]),
                {ok, nil};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Failed to store VM: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception storing VM: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Get a VM from the store
get_vm(_Store, Id) ->
    try
        io:format("~n[KHEPRI] Getting VM with ID: ~p~n", [Id]),
        
        case khepri:get(vm_path(Id)) of
            {ok, [VM]} -> 
                io:format("[KHEPRI] VM found~n"),
                {ok, VM};
            {error, {khepri, mismatching_node, _}} -> 
                io:format("[KHEPRI] VM not found~n"),
                {error, not_found};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error getting VM: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception getting VM: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% List all VMs
list_vms(_Store) ->
    try
        io:format("~n[KHEPRI] Listing all VMs~n"),
        
        case khepri:get_many(?VMS_PATH) of
            {ok, Tree} ->
                % Extract values from tree
                VMs = maps:values(Tree),
                io:format("[KHEPRI] Found ~p VMs~n", [length(VMs)]),
                {ok, VMs};
            {error, {khepri, mismatching_node, _}} ->
                io:format("[KHEPRI] No VMs found~n"),
                {ok, []};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error listing VMs: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception listing VMs: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Delete a VM
delete_vm(_Store, Id) ->
    try
        io:format("~n[KHEPRI] Deleting VM with ID: ~p~n", [Id]),
        
        case khepri:delete(vm_path(Id)) of
            ok ->
                io:format("[KHEPRI] VM deleted successfully~n"),
                {ok, nil};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error deleting VM: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception deleting VM: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Put a Host into the store
put_host(_Store, Host) ->
    try
        % Extract ID from Host tuple
        Id = element(2, Host),
        
        io:format("~n[KHEPRI] Storing Host in Khepri:~n"),
        io:format("  - Host ID: ~p~n", [Id]),
        
        % Store the Host using Khepri
        case khepri:put(host_path(Id), Host) of
            ok ->
                io:format("[KHEPRI] Host ~p stored successfully~n", [Id]),
                {ok, nil};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Failed to store Host: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception storing Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Get a Host from the store
get_host(_Store, Id) ->
    try
        io:format("~n[KHEPRI] Getting Host with ID: ~p~n", [Id]),
        
        case khepri:get(host_path(Id)) of
            {ok, [Host]} -> 
                io:format("[KHEPRI] Host found~n"),
                {ok, Host};
            {error, {khepri, mismatching_node, _}} -> 
                io:format("[KHEPRI] Host not found~n"),
                {error, not_found};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error getting Host: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception getting Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% List all Hosts
list_hosts(_Store) ->
    try
        io:format("~n[KHEPRI] Listing all Hosts~n"),
        
        case khepri:get_many(?HOSTS_PATH) of
            {ok, Tree} ->
                % Extract values from tree
                Hosts = maps:values(Tree),
                io:format("[KHEPRI] Found ~p Hosts~n", [length(Hosts)]),
                {ok, Hosts};
            {error, {khepri, mismatching_node, _}} ->
                io:format("[KHEPRI] No Hosts found~n"),
                {ok, []};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error listing Hosts: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception listing Hosts: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Delete a Host
delete_host(_Store, Id) ->
    try
        io:format("~n[KHEPRI] Deleting Host with ID: ~p~n", [Id]),
        
        case khepri:delete(host_path(Id)) of
            ok ->
                io:format("[KHEPRI] Host deleted successfully~n"),
                {ok, nil};
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error deleting Host: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception deleting Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Update VM state
update_vm_state(_Store, Id, State) ->
    try
        io:format("~n[KHEPRI] Updating VM state in Khepri:~n"),
        io:format("  - VM ID: ~p~n", [Id]),
        io:format("  - New State: ~p~n", [State]),
        
        % Get the current VM
        case khepri:get(vm_path(Id)) of
            {ok, [VM]} ->
                % Extract old state
                OldState = element(7, VM),
                io:format("  - Old State: ~p~n", [OldState]),
                
                % Update the state in the VM tuple
                UpdatedVM = setelement(7, VM, State),
                
                % Store the updated VM
                case khepri:put(vm_path(Id), UpdatedVM) of
                    ok ->
                        io:format("[KHEPRI] VM state updated successfully~n"),
                        {ok, nil};
                    {error, Reason} ->
                        io:format("[KHEPRI ERROR] Failed to update VM state: ~p~n", [Reason]),
                        {error, {storage_error, io_lib:format("~p", [Reason])}}
                end;
                
            {error, {khepri, mismatching_node, _}} ->
                io:format("[KHEPRI] VM not found for state update~n"),
                {error, not_found};
                
            {error, Reason} ->
                io:format("[KHEPRI ERROR] Error getting VM for state update: ~p~n", [Reason]),
                {error, {storage_error, io_lib:format("~p", [Reason])}}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception updating VM state: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.

% Assign VM to Host
assign_vm_to_host(_Store, VmId, HostId) ->
    try
        io:format("~n[KHEPRI] Assigning VM to Host:~n"),
        io:format("  - VM ID: ~p~n", [VmId]),
        io:format("  - Host ID: ~p~n", [HostId]),
        
        % Get the current VM
        VMResult = khepri:get(vm_path(VmId)),
        % Get the current Host
        HostResult = khepri:get(host_path(HostId)),
        
        case {VMResult, HostResult} of
            {{ok, [VM]}, {ok, [Host]}} ->
                % Update VM host_id (position 8)
                OldHostId = element(8, VM),
                io:format("  - Old Host ID: ~p~n", [OldHostId]),
                UpdatedVM = setelement(8, VM, {some, HostId}),
                
                % Update Host vm_ids list (position 9)
                VmIds = element(9, Host),
                NewVmIds = case lists:member(VmId, VmIds) of
                    true -> VmIds;
                    false -> [VmId | VmIds]
                end,
                UpdatedHost = setelement(9, Host, NewVmIds),
                
                % Store both updated records
                VMPutResult = khepri:put(vm_path(VmId), UpdatedVM),
                HostPutResult = khepri:put(host_path(HostId), UpdatedHost),
                
                case {VMPutResult, HostPutResult} of
                    {ok, ok} ->
                        io:format("[KHEPRI] VM and Host updated successfully~n"),
                        {ok, nil};
                    _ ->
                        io:format("[KHEPRI ERROR] Failed to update VM or Host~n"),
                        {error, {storage_error, "Failed to update VM or Host"}}
                end;
                
            {{error, _}, _} ->
                io:format("[KHEPRI ERROR] VM not found for assignment~n"),
                {error, not_found};
                
            {_, {error, _}} ->
                io:format("[KHEPRI ERROR] Host not found for assignment~n"),
                {error, not_found}
        end
    catch
        _:Error ->
            io:format("[KHEPRI ERROR] Exception assigning VM to Host: ~p~n", [Error]),
            {error, {storage_error, io_lib:format("~p", [Error])}}
    end.
