-module(blixard_khepri_store).
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

% These are all simple mock implementations that just log what they would do
% and return success values. They don't depend on any external libraries.

% Start the Khepri store
start(_Nodes, ClusterName) ->
    io:format("MOCK: Starting Khepri store with name: ~p~n", [ClusterName]),
    {ok, {khepri, ClusterName}}.

% Stop the Khepri store
stop(_Store) ->
    io:format("MOCK: Stopping Khepri store~n"),
    {ok, nil}.

% Put a VM into the store
put_vm(_Store, VM) ->
    Id = element(2, VM),  % Get ID from tuple position 2
    io:format("MOCK: Storing VM with ID: ~p~n", [Id]),
    {ok, nil}.

% Get a VM from the store
get_vm(_Store, Id) ->
    io:format("MOCK: Getting VM with ID: ~p~n", [Id]),
    % For now, just return not_found to test error handling
    {error, not_found}.

% List all VMs
list_vms(_Store) ->
    io:format("MOCK: Listing all VMs~n"),
    % Return empty list for now
    {ok, []}.

% Delete a VM
delete_vm(_Store, Id) ->
    io:format("MOCK: Deleting VM with ID: ~p~n", [Id]),
    {ok, nil}.

% Put a Host into the store
put_host(_Store, Host) ->
    Id = element(2, Host),  % Get ID from tuple position 2
    io:format("MOCK: Storing Host with ID: ~p~n", [Id]),
    {ok, nil}.

% Get a Host from the store
get_host(_Store, Id) ->
    io:format("MOCK: Getting Host with ID: ~p~n", [Id]),
    % For now, just return not_found to test error handling
    {error, not_found}.

% List all Hosts
list_hosts(_Store) ->
    io:format("MOCK: Listing all Hosts~n"),
    % Return empty list for now
    {ok, []}.

% Delete a Host
delete_host(_Store, Id) ->
    io:format("MOCK: Deleting Host with ID: ~p~n", [Id]),
    {ok, nil}.

% Update VM state
update_vm_state(_Store, Id, State) ->
    io:format("MOCK: Updating VM state for ID ~p to ~p~n", [Id, State]),
    {ok, nil}.

% Assign VM to Host
assign_vm_to_host(_Store, VmId, HostId) ->
    io:format("MOCK: Assigning VM ~p to Host ~p~n", [VmId, HostId]),
    {ok, nil}.
