%% src/node_manager_helper.erl
-module(node_manager_helper).
-export([remote_stop/1, kill_processes/1, remove_directory/1, get_khepri_directories/0]).

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
                        {error, list_to_binary(io_lib:format("RPC failed: ~p", [Reason1]))};
                    _ ->
                        {ok, nil}
                end;
            false ->
                {error, <<"Failed to connect to node">>}
        end
    catch
        error:Error ->
            {error, list_to_binary(io_lib:format("Error: ~p", [Error]))}
    end.

%% Kill processes by name pattern using pkill
kill_processes(Pattern) when is_binary(Pattern) ->
    kill_processes(binary_to_list(Pattern));
kill_processes(Pattern) when is_list(Pattern) ->
    try
        % Get current process PID to exclude it
        CurrentPid = os:getpid(),
        % Use pgrep to find PIDs, exclude current one, then kill them
        FindCommand = "pgrep -f " ++ Pattern ++ " | grep -v " ++ CurrentPid,
        case os:cmd(FindCommand) of
            [] ->
                {ok, nil};  % No processes found (or only current process)
            PidList ->
                % Kill each PID individually
                Pids = string:tokens(PidList, "\n"),
                lists:foreach(fun(Pid) ->
                    case Pid of
                        [] -> ok;  % Skip empty lines
                        _ -> os:cmd("kill " ++ Pid)
                    end
                end, Pids),
                {ok, nil}
        end
    catch
        error:Error ->
            {error, list_to_binary(io_lib:format("Failed to kill processes: ~p", [Error]))}
    end.

%% Remove directory recursively using rm -rf
remove_directory(Path) when is_binary(Path) ->
    remove_directory(binary_to_list(Path));
remove_directory(Path) when is_list(Path) ->
    try
        % Ensure path starts with khepri to prevent accidental deletion
        case string:str(Path, "khepri") of
            0 -> {error, <<"Path does not contain 'khepri' - safety check failed">>};
            _ ->
                Command = "rm -rf '" ++ Path ++ "'",
                case os:cmd(Command) of
                    [] -> {ok, nil};  % rm returns empty string on success
                    Output ->
                        case string:find(Output, "No such file") of
                            nomatch -> {error, list_to_binary(Output)};
                            _ -> {ok, nil}  % File already doesn't exist
                        end
                end
        end
    catch
        error:Error ->
            {error, list_to_binary(io_lib:format("Failed to remove directory: ~p", [Error]))}
    end.

%% Get list of Khepri data directories
get_khepri_directories() ->
    try
        Command = "find . -maxdepth 1 -name 'khepri*' -type d",
        Output = os:cmd(Command),
        case Output of
            [] -> {ok, []};
            _ ->
                % Split output by newlines and filter out empty strings
                Lines = string:tokens(string:trim(Output), "\n"),
                Dirs = lists:filter(fun(Line) -> Line =/= [] end, Lines),
                % Convert to binaries for Gleam compatibility
                BinaryDirs = lists:map(fun(Dir) -> list_to_binary(Dir) end, Dirs),
                {ok, BinaryDirs}
        end
    catch
        error:Error ->
            {error, list_to_binary(io_lib:format("Failed to list directories: ~p", [Error]))}
    end.
