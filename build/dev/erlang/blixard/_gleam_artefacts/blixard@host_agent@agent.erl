-module(blixard@host_agent@agent).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([start/2]).
-export_type([command/0, state/0, vm_process/0, host_status/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/host_agent/agent.gleam\n").

-type command() :: {start_vm,
        binary(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {stop_vm,
        binary(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {pause_vm,
        binary(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {resume_vm,
        binary(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {update_resources,
        blixard@domain@types:resources(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {set_schedulable,
        boolean(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {get_status,
        gleam@erlang@process:subject({ok, host_status()} | {error, binary()})}.

-type state() :: {state,
        binary(),
        blixard@storage@khepri_store:khepri(),
        gleam@dict:dict(binary(), vm_process()),
        blixard@domain@types:resources(),
        boolean()}.

-type vm_process() :: {vm_process,
        binary(),
        gleam@option:option(gleam@erlang@process:pid_()),
        blixard@domain@types:resource_state()}.

-type host_status() :: {host_status,
        binary(),
        integer(),
        list(binary()),
        blixard@domain@types:resources(),
        boolean()}.

-file("src/blixard/host_agent/agent.gleam", 97).
-spec debug_actor_error(gleam@otp@actor:start_error()) -> binary().
debug_actor_error(Err) ->
    <<"Actor start error"/utf8>>.

-file("src/blixard/host_agent/agent.gleam", 474).
-spec handle_get_status(
    gleam@erlang@process:subject({ok, host_status()} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_get_status(Reply_with, State) ->
    Running_vms = begin
        _pipe = erlang:element(4, State),
        _pipe@1 = gleam@dict:filter(
            _pipe,
            fun(_, Vm_process) -> erlang:element(4, Vm_process) =:= running end
        ),
        maps:keys(_pipe@1)
    end,
    Status = {host_status,
        erlang:element(2, State),
        maps:size(erlang:element(4, State)),
        Running_vms,
        erlang:element(5, State),
        erlang:element(6, State)},
    gleam@erlang@process:send(Reply_with, {ok, Status}),
    gleam@otp@actor:continue(State).

-file("src/blixard/host_agent/agent.gleam", 499).
?DOC(" Helper function to debug KhepriError\n").
-spec debug_error(blixard@storage@khepri_store:khepri_error()) -> binary().
debug_error(Error) ->
    case Error of
        {connection_error, Msg} ->
            <<"Connection error: "/utf8, Msg/binary>>;

        {consensus_error, Msg@1} ->
            <<"Consensus error: "/utf8, Msg@1/binary>>;

        {storage_error, Msg@2} ->
            <<"Storage error: "/utf8, Msg@2/binary>>;

        not_found ->
            <<"Resource not found"/utf8>>;

        {invalid_data, Msg@3} ->
            <<"Invalid data: "/utf8, Msg@3/binary>>
    end.

-file("src/blixard/host_agent/agent.gleam", 170).
-spec start_vm_process(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
start_vm_process(Vm_id, Reply_with, State) ->
    State_result = blixard_khepri_store:update_vm_state(
        erlang:element(3, State),
        Vm_id,
        provisioning
    ),
    case State_result of
        {ok, _} ->
            gleam_stdlib:println(
                <<<<<<"Starting VM "/utf8, Vm_id/binary>>/binary,
                        " on host "/utf8>>/binary,
                    (erlang:element(2, State))/binary>>
            ),
            case blixard_khepri_store:update_vm_state(
                erlang:element(3, State),
                Vm_id,
                running
            ) of
                {ok, _} ->
                    New_vm_process = {vm_process,
                        Vm_id,
                        {some, erlang:self()},
                        running},
                    New_vms = gleam@dict:insert(
                        erlang:element(4, State),
                        Vm_id,
                        New_vm_process
                    ),
                    New_state = begin
                        _record = State,
                        {state,
                            erlang:element(2, _record),
                            erlang:element(3, _record),
                            New_vms,
                            erlang:element(5, _record),
                            erlang:element(6, _record)}
                    end,
                    gleam@erlang@process:send(Reply_with, {ok, nil}),
                    gleam@otp@actor:continue(New_state);

                {error, Err} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Failed to update VM state: "/utf8,
                                (debug_error(Err))/binary>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Failed to update VM state: "/utf8,
                        (debug_error(Err@1))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/host_agent/agent.gleam", 117).
-spec handle_start_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_start_vm(Vm_id, Reply_with, State) ->
    case gleam_stdlib:map_get(erlang:element(4, State), Vm_id) of
        {ok, Vm_process} ->
            case erlang:element(4, Vm_process) of
                running ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM is already running"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State);

                _ ->
                    start_vm_process(Vm_id, Reply_with, State)
            end;

        {error, _} ->
            case blixard_khepri_store:get_vm(erlang:element(3, State), Vm_id) of
                {ok, Vm} ->
                    case erlang:element(8, Vm) of
                        {some, Id} ->
                            case Id =:= erlang:element(2, State) of
                                true ->
                                    start_vm_process(Vm_id, Reply_with, State);

                                false ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        {error,
                                            <<"VM is not assigned to this host"/utf8>>}
                                    ),
                                    gleam@otp@actor:continue(State)
                            end;

                        none ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"VM is not assigned to this host"/utf8>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                {error, _} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM not found"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end
    end.

-file("src/blixard/host_agent/agent.gleam", 234).
-spec handle_stop_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_stop_vm(Vm_id, Reply_with, State) ->
    case gleam_stdlib:map_get(erlang:element(4, State), Vm_id) of
        {ok, Vm_process} ->
            State_result = blixard_khepri_store:update_vm_state(
                erlang:element(3, State),
                Vm_id,
                stopping
            ),
            case State_result of
                {ok, _} ->
                    gleam_stdlib:println(
                        <<<<<<"Stopping VM "/utf8, Vm_id/binary>>/binary,
                                " on host "/utf8>>/binary,
                            (erlang:element(2, State))/binary>>
                    ),
                    case blixard_khepri_store:update_vm_state(
                        erlang:element(3, State),
                        Vm_id,
                        stopped
                    ) of
                        {ok, _} ->
                            New_vms = gleam@dict:delete(
                                erlang:element(4, State),
                                Vm_id
                            ),
                            New_state = begin
                                _record = State,
                                {state,
                                    erlang:element(2, _record),
                                    erlang:element(3, _record),
                                    New_vms,
                                    erlang:element(5, _record),
                                    erlang:element(6, _record)}
                            end,
                            gleam@erlang@process:send(Reply_with, {ok, nil}),
                            gleam@otp@actor:continue(New_state);

                        {error, Err} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Failed to update VM state: "/utf8,
                                        (debug_error(Err))/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                {error, Err@1} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Failed to update VM state: "/utf8,
                                (debug_error(Err@1))/binary>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, _} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM is not running on this host"/utf8>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/host_agent/agent.gleam", 290).
-spec handle_pause_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_pause_vm(Vm_id, Reply_with, State) ->
    case gleam_stdlib:map_get(erlang:element(4, State), Vm_id) of
        {ok, Vm_process} ->
            case erlang:element(4, Vm_process) of
                running ->
                    State_result = blixard_khepri_store:update_vm_state(
                        erlang:element(3, State),
                        Vm_id,
                        paused
                    ),
                    case State_result of
                        {ok, _} ->
                            New_vm_process = begin
                                _record = Vm_process,
                                {vm_process,
                                    erlang:element(2, _record),
                                    erlang:element(3, _record),
                                    paused}
                            end,
                            New_vms = gleam@dict:insert(
                                erlang:element(4, State),
                                Vm_id,
                                New_vm_process
                            ),
                            New_state = begin
                                _record@1 = State,
                                {state,
                                    erlang:element(2, _record@1),
                                    erlang:element(3, _record@1),
                                    New_vms,
                                    erlang:element(5, _record@1),
                                    erlang:element(6, _record@1)}
                            end,
                            gleam@erlang@process:send(Reply_with, {ok, nil}),
                            gleam@otp@actor:continue(New_state);

                        {error, Err} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Failed to update VM state: "/utf8,
                                        (debug_error(Err))/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                _ ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM is not in a running state"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, _} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM is not running on this host"/utf8>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/host_agent/agent.gleam", 340).
-spec handle_resume_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_resume_vm(Vm_id, Reply_with, State) ->
    case gleam_stdlib:map_get(erlang:element(4, State), Vm_id) of
        {ok, Vm_process} ->
            case erlang:element(4, Vm_process) of
                paused ->
                    State_result = blixard_khepri_store:update_vm_state(
                        erlang:element(3, State),
                        Vm_id,
                        running
                    ),
                    case State_result of
                        {ok, _} ->
                            New_vm_process = begin
                                _record = Vm_process,
                                {vm_process,
                                    erlang:element(2, _record),
                                    erlang:element(3, _record),
                                    running}
                            end,
                            New_vms = gleam@dict:insert(
                                erlang:element(4, State),
                                Vm_id,
                                New_vm_process
                            ),
                            New_state = begin
                                _record@1 = State,
                                {state,
                                    erlang:element(2, _record@1),
                                    erlang:element(3, _record@1),
                                    New_vms,
                                    erlang:element(5, _record@1),
                                    erlang:element(6, _record@1)}
                            end,
                            gleam@erlang@process:send(Reply_with, {ok, nil}),
                            gleam@otp@actor:continue(New_state);

                        {error, Err} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Failed to update VM state: "/utf8,
                                        (debug_error(Err))/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                _ ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM is not in a paused state"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, _} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM is not running on this host"/utf8>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/host_agent/agent.gleam", 390).
-spec handle_update_resources(
    blixard@domain@types:resources(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_update_resources(Resources, Reply_with, State) ->
    Host_result = blixard_khepri_store:get_host(
        erlang:element(3, State),
        erlang:element(2, State)
    ),
    case Host_result of
        {ok, Host} ->
            Updated_host = begin
                _record = Host,
                {host,
                    erlang:element(2, _record),
                    erlang:element(3, _record),
                    erlang:element(4, _record),
                    erlang:element(5, _record),
                    erlang:element(6, _record),
                    Resources,
                    erlang:element(8, _record),
                    erlang:element(9, _record),
                    erlang:element(10, _record),
                    erlang:element(11, _record),
                    erlang:element(12, _record),
                    erlang:element(13, _record),
                    erlang:element(14, _record)}
            end,
            case blixard_khepri_store:put_host(
                erlang:element(3, State),
                Updated_host
            ) of
                {ok, _} ->
                    New_state = begin
                        _record@1 = State,
                        {state,
                            erlang:element(2, _record@1),
                            erlang:element(3, _record@1),
                            erlang:element(4, _record@1),
                            Resources,
                            erlang:element(6, _record@1)}
                    end,
                    gleam@erlang@process:send(Reply_with, {ok, nil}),
                    gleam@otp@actor:continue(New_state);

                {error, Err} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Failed to update host: "/utf8,
                                (debug_error(Err))/binary>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Failed to retrieve host: "/utf8,
                        (debug_error(Err@1))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/host_agent/agent.gleam", 432).
-spec handle_set_schedulable(
    boolean(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_set_schedulable(Schedulable, Reply_with, State) ->
    Host_result = blixard_khepri_store:get_host(
        erlang:element(3, State),
        erlang:element(2, State)
    ),
    case Host_result of
        {ok, Host} ->
            Updated_host = begin
                _record = Host,
                {host,
                    erlang:element(2, _record),
                    erlang:element(3, _record),
                    erlang:element(4, _record),
                    erlang:element(5, _record),
                    erlang:element(6, _record),
                    erlang:element(7, _record),
                    erlang:element(8, _record),
                    erlang:element(9, _record),
                    Schedulable,
                    erlang:element(11, _record),
                    erlang:element(12, _record),
                    erlang:element(13, _record),
                    erlang:element(14, _record)}
            end,
            case blixard_khepri_store:put_host(
                erlang:element(3, State),
                Updated_host
            ) of
                {ok, _} ->
                    New_state = begin
                        _record@1 = State,
                        {state,
                            erlang:element(2, _record@1),
                            erlang:element(3, _record@1),
                            erlang:element(4, _record@1),
                            erlang:element(5, _record@1),
                            Schedulable}
                    end,
                    gleam@erlang@process:send(Reply_with, {ok, nil}),
                    gleam@otp@actor:continue(New_state);

                {error, Err} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Failed to update host: "/utf8,
                                (debug_error(Err))/binary>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Failed to retrieve host: "/utf8,
                        (debug_error(Err@1))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/host_agent/agent.gleam", 102).
-spec handle_message(command(), state()) -> gleam@otp@actor:next(command(), state()).
handle_message(Message, State) ->
    case Message of
        {start_vm, Vm_id, Reply_with} ->
            handle_start_vm(Vm_id, Reply_with, State);

        {stop_vm, Vm_id@1, Reply_with@1} ->
            handle_stop_vm(Vm_id@1, Reply_with@1, State);

        {pause_vm, Vm_id@2, Reply_with@2} ->
            handle_pause_vm(Vm_id@2, Reply_with@2, State);

        {resume_vm, Vm_id@3, Reply_with@3} ->
            handle_resume_vm(Vm_id@3, Reply_with@3, State);

        {update_resources, Resources, Reply_with@4} ->
            handle_update_resources(Resources, Reply_with@4, State);

        {set_schedulable, Schedulable, Reply_with@5} ->
            handle_set_schedulable(Schedulable, Reply_with@5, State);

        {get_status, Reply_with@6} ->
            handle_get_status(Reply_with@6, State)
    end.

-file("src/blixard/host_agent/agent.gleam", 67).
-spec start(binary(), blixard@storage@khepri_store:khepri()) -> {ok,
        gleam@erlang@process:subject(command())} |
    {error, binary()}.
start(Host_id, Store) ->
    Host_result = blixard_khepri_store:get_host(Store, Host_id),
    case Host_result of
        {ok, Host} ->
            Initial_state = {state,
                Host_id,
                Store,
                maps:new(),
                erlang:element(7, Host),
                erlang:element(10, Host)},
            case gleam@otp@actor:start(Initial_state, fun handle_message/2) of
                {ok, Actor} ->
                    {ok, Actor};

                {error, Err} ->
                    {error,
                        <<"Failed to start actor: "/utf8,
                            (debug_actor_error(Err))/binary>>}
            end;

        {error, Err@1} ->
            {error,
                <<"Failed to retrieve host: "/utf8,
                    (debug_error(Err@1))/binary>>}
    end.
