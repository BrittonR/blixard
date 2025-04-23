%% src/distribution_helper.erl
-module(distribution_helper).
-export([start_distribution/3]).

start_distribution(Name, Host, Cookie) ->
    NodeName = list_to_atom(Name ++ "@" ++ Host),
    CookieAtom = list_to_atom(Cookie),
    
    % First, set the cookie
    erlang:set_cookie(node(), CookieAtom),
    
    % Try starting with longnames
    case net_kernel:start([NodeName, longnames]) of
        {ok, _} -> 
            io:format("Started distribution with longnames~n");
        {error, {already_started, _}} ->
            io:format("Distribution already started~n");
        Error ->
            io:format("Failed with longnames: ~p, trying shortnames~n", [Error]),
            % Try with shortnames as fallback
            case net_kernel:start([NodeName, shortnames]) of
                {ok, _} -> 
                    io:format("Started distribution with shortnames~n");
                {error, {already_started, _}} ->
                    io:format("Distribution already started~n");
                Error2 ->
                    io:format("Failed to start distribution: ~p~n", [Error2])
            end
    end.
