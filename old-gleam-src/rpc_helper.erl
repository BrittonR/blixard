-module(rpc_helper).
-export([call_with_timeout/5]).

%% Call RPC with timeout to prevent hanging
call_with_timeout(Node, Module, Function, Args, Timeout) ->
    case rpc:call(Node, Module, Function, Args, Timeout) of
        {badrpc, timeout} ->
            {error, <<"RPC call timed out">>};
        {badrpc, Reason} ->
            {error, list_to_binary(io_lib:format("RPC failed: ~p", [Reason]))};
        Result ->
            {ok, Result}
    end.