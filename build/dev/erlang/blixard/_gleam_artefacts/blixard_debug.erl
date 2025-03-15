-module(blixard_debug).
-export([print_vm/1, create_module/0]).

% Print the structure of a VM
print_vm(VM) ->
    io:format("~nVM Type: ~p~n", [element(1, VM)]),
    io:format("VM ID: ~p~n", [element(2, VM)]),
    io:format("VM Structure:~n~p~n", [VM]),
    nil.

% Create a dynamic module for blixard_khepri_store if it doesn't exist
create_module() ->
    case code:which(blixard_khepri_store) of
        non_existing ->
            % Create a simple version of the module
            Module = "-module(blixard_khepri_store).
                    -export([start/2, stop/1, put_vm/2, get_vm/2, list_vms/1, delete_vm/2,
                            put_host/2, get_host/2, list_hosts/1, delete_host/2,
                            update_vm_state/3, assign_vm_to_host/3]).

                    start(_Nodes, ClusterName) ->
                        io:format(\"~nDEBUG: start called with ~p~n\", [ClusterName]),
                        {ok, {khepri, ClusterName}}.

                    stop(_Store) ->
                        io:format(\"~nDEBUG: stop called~n\"),
                        {ok, nil}.

                    put_vm(_Store, VM) ->
                        io:format(\"~nDEBUG: put_vm called with:~n~p~n\", [VM]),
                        % Extract ID - assuming it's at element 2
                        Id = element(2, VM),
                        io:format(\"~nDEBUG: VM ID: ~p~n\", [Id]),
                        {ok, nil}.

                    get_vm(_Store, Id) ->
                        io:format(\"~nDEBUG: get_vm called with ID: ~p~n\", [Id]),
                        {error, not_found}.

                    list_vms(_Store) ->
                        io:format(\"~nDEBUG: list_vms called~n\"),
                        {ok, []}.

                    delete_vm(_Store, Id) ->
                        io:format(\"~nDEBUG: delete_vm called with ID: ~p~n\", [Id]),
                        {ok, nil}.

                    put_host(_Store, Host) ->
                        io:format(\"~nDEBUG: put_host called with:~n~p~n\", [Host]),
                        {ok, nil}.

                    get_host(_Store, Id) ->
                        io:format(\"~nDEBUG: get_host called with ID: ~p~n\", [Id]),
                        {error, not_found}.

                    list_hosts(_Store) ->
                        io:format(\"~nDEBUG: list_hosts called~n\"),
                        {ok, []}.

                    delete_host(_Store, Id) ->
                        io:format(\"~nDEBUG: delete_host called with ID: ~p~n\", [Id]),
                        {ok, nil}.

                    update_vm_state(_Store, Id, State) ->
                        io:format(\"~nDEBUG: update_vm_state called with ID: ~p, State: ~p~n\", [Id, State]),
                        {ok, nil}.

                    assign_vm_to_host(_Store, VmId, HostId) ->
                        io:format(\"~nDEBUG: assign_vm_to_host called with VM ID: ~p, Host ID: ~p~n\", [VmId, HostId]),
                        {ok, nil}.
                    ",
            
            % Write the module to a file
            FileName = "blixard_khepri_store.erl",
            file:write_file(FileName, Module),
            
            % Compile it
            compile:file(FileName),
            
            io:format("Created and compiled blixard_khepri_store module~n");
        _ ->
            io:format("blixard_khepri_store module already exists~n")
    end,
    nil.
