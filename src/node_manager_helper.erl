%% src/node_manager_helper.erl
-module(node_manager_helper).
-export([remote_stop/1]).

%% Send a shutdown signal to a remote node
remote_stop(NodeName) when is_binary(NodeName) ->
    remote_stop(binary_to_list(NodeName));
remote_stop(NodeName) when is_list(NodeName) ->
    try
        % Convert string to atom
        NodeAtom = list_to_atom(NodeName),
        
        % Try to connect to the node
        case net_kernel:connect_node(NodeAtom) of
            true ->
                % Send stop signal using RPC
                case rpc:call(NodeAtom, init, stop, []) of
                    {badrpc, Reason1} ->
                        {error, io_lib:format("RPC failed: ~p", [Reason1])};
                    _ ->
                        {ok, nil}
                end;
            false ->
                {error, "Failed to connect to node"}
        end
    catch
        error:Error -> 
            {error, io_lib:format("Error: ~p", [Error])}
    end.
