%% src/distribution_helper.erl
-module(distribution_helper).
-export([start_distribution/3]).

start_distribution(Name, Host, Cookie) ->
    % Convert inputs to strings if they're not already
    NameStr = to_list(Name),
    HostStr = to_list(Host),
    CookieStr = to_list(Cookie),
    
    % Create node name
    NodeStr = NameStr ++ "@" ++ HostStr,
    NodeName = list_to_atom(NodeStr),
    CookieAtom = list_to_atom(CookieStr),
    
    % IMPORTANT: First start the distribution, then set the cookie
    
    % Try starting with longnames
    StartResult = case net_kernel:start([NodeName, longnames]) of
        {ok, _Pid} -> 
            io:format("Started distribution with longnames~n"),
            ok;
        {error, {already_started, _}} ->
            io:format("Distribution already started~n"),
            ok;
        Error ->
            io:format("Failed with longnames: ~p, trying shortnames~n", [Error]),
            % Try with shortnames as fallback
            case net_kernel:start([NodeName, shortnames]) of
                {ok, _} -> 
                    io:format("Started distribution with shortnames~n"),
                    ok;
                {error, {already_started, _}} ->
                    io:format("Distribution already started~n"),
                    ok;
                Error2 ->
                    io:format("Failed to start distribution: ~p~n", [Error2]),
                    error
            end
    end,
    
    % Now that distribution is started, set the cookie
    case StartResult of
        ok ->
            erlang:set_cookie(node(), CookieAtom),
            io:format("Set cookie to ~p~n", [CookieAtom]);
        _ ->
            io:format("Skipping cookie setup because distribution failed to start~n")
    end.

% Helper to convert to list (string)
to_list(X) when is_list(X) -> X;
to_list(X) when is_atom(X) -> atom_to_list(X);
to_list(X) when is_binary(X) -> binary_to_list(X);
to_list(X) -> lists:flatten(io_lib:format("~p", [X])).
