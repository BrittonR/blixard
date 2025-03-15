-module(blixard@orchestrator@core).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([start/1]).
-export_type([command/0, state/0, system_status/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/orchestrator/core.gleam\n").

-type command() :: {create_vm,
        blixard@domain@types:micro_vm(),
        gleam@erlang@process:subject({ok, blixard@domain@types:micro_vm()} |
            {error, binary()})} |
    {delete_vm,
        binary(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {start_vm,
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
    {register_host,
        blixard@domain@types:host(),
        gleam@erlang@process:subject({ok, blixard@domain@types:host()} |
            {error, binary()})} |
    {deregister_host,
        binary(),
        gleam@erlang@process:subject({ok, nil} | {error, binary()})} |
    {get_system_status,
        gleam@erlang@process:subject({ok, system_status()} | {error, binary()})}.

-type state() :: {state,
        blixard@storage@khepri_store:khepri(),
        gleam@dict:dict(binary(), gleam@erlang@process:subject(blixard@host_agent@agent:command())),
        blixard@scheduler@scheduler:scheduling_strategy()}.

-type system_status() :: {system_status,
        integer(),
        integer(),
        gleam@dict:dict(blixard@domain@types:resource_state(), integer())}.

-file("src/blixard/orchestrator/core.gleam", 79).
-spec debug_actor_error(gleam@otp@actor:start_error()) -> binary().
debug_actor_error(Err) ->
    <<"Actor start error"/utf8>>.

-file("src/blixard/orchestrator/core.gleam", 158).
-spec validate_vm(blixard@domain@types:micro_vm()) -> {ok, nil} |
    {error, binary()}.
validate_vm(Vm) ->
    case {erlang:element(3, Vm), erlang:element(6, Vm)} of
        {<<""/utf8>>, _} ->
            {error, <<"VM name cannot be empty"/utf8>>};

        {_, Resources} when erlang:element(2, Resources) =< 0 ->
            {error, <<"CPU cores must be positive"/utf8>>};

        {_, Resources@1} when erlang:element(3, Resources@1) =< 0 ->
            {error, <<"Memory must be positive"/utf8>>};

        {_, Resources@2} when erlang:element(4, Resources@2) =< 0 ->
            {error, <<"Disk space must be positive"/utf8>>};

        {_, _} ->
            {ok, nil}
    end.

-file("src/blixard/orchestrator/core.gleam", 508).
-spec validate_host(blixard@domain@types:host()) -> {ok, nil} |
    {error, binary()}.
validate_host(Host) ->
    case {erlang:element(3, Host), erlang:element(5, Host)} of
        {<<""/utf8>>, _} ->
            {error, <<"Host name cannot be empty"/utf8>>};

        {_, <<""/utf8>>} ->
            {error, <<"Host control IP cannot be empty"/utf8>>};

        {_, _} ->
            {ok, nil}
    end.

-file("src/blixard/orchestrator/core.gleam", 625).
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

-file("src/blixard/orchestrator/core.gleam", 101).
-spec handle_create_vm(
    blixard@domain@types:micro_vm(),
    gleam@erlang@process:subject({ok, blixard@domain@types:micro_vm()} |
        {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_create_vm(Vm, Reply_with, State) ->
    case validate_vm(Vm) of
        {ok, _} ->
            Vm_with_state = begin
                _record = Vm,
                {micro_vm,
                    erlang:element(2, _record),
                    erlang:element(3, _record),
                    erlang:element(4, _record),
                    erlang:element(5, _record),
                    erlang:element(6, _record),
                    pending,
                    erlang:element(8, _record),
                    erlang:element(9, _record),
                    erlang:element(10, _record),
                    erlang:element(11, _record),
                    erlang:element(12, _record),
                    erlang:element(13, _record),
                    erlang:element(14, _record),
                    erlang:element(15, _record)}
            end,
            case blixard_khepri_store:put_vm(
                erlang:element(2, State),
                Vm_with_state
            ) of
                {ok, _} ->
                    case blixard@scheduler@scheduler:schedule_vm(
                        erlang:element(2, State),
                        erlang:element(2, Vm),
                        erlang:element(4, State)
                    ) of
                        {scheduled, Vm_id, Host_id} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {ok, Vm_with_state}
                            ),
                            gleam@otp@actor:continue(State);

                        {no_suitable_host, Reason} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"No suitable host found: "/utf8,
                                        Reason/binary>>}
                            ),
                            gleam@otp@actor:continue(State);

                        {scheduling_error, Error} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Scheduling error: "/utf8, Error/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                {error, Err} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Failed to store VM: "/utf8,
                                (debug_error(Err))/binary>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Reason@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"Invalid VM configuration: "/utf8, Reason@1/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 172).
-spec handle_delete_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_delete_vm(Vm_id, Reply_with, State) ->
    case blixard_khepri_store:get_vm(erlang:element(2, State), Vm_id) of
        {ok, Vm} ->
            case erlang:element(7, Vm) of
                running ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Cannot delete running VM. Stop it first."/utf8>>}
                    ),
                    gleam@otp@actor:continue(State);

                paused ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Cannot delete running VM. Stop it first."/utf8>>}
                    ),
                    gleam@otp@actor:continue(State);

                _ ->
                    case blixard_khepri_store:delete_vm(
                        erlang:element(2, State),
                        Vm_id
                    ) of
                        {ok, _} ->
                            gleam@erlang@process:send(Reply_with, {ok, nil}),
                            gleam@otp@actor:continue(State);

                        {error, Err} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Failed to delete VM: "/utf8,
                                        (debug_error(Err))/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end
            end;

        {error, Err@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM not found: "/utf8, (debug_error(Err@1))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 218).
-spec handle_start_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_start_vm(Vm_id, Reply_with, State) ->
    case blixard_khepri_store:get_vm(erlang:element(2, State), Vm_id) of
        {ok, Vm} ->
            case erlang:element(8, Vm) of
                {some, Id} ->
                    case gleam_stdlib:map_get(erlang:element(3, State), Id) of
                        {ok, Host_agent} ->
                            Host_reply = gleam@erlang@process:new_subject(),
                            gleam@erlang@process:send(
                                Host_agent,
                                {start_vm, Vm_id, Host_reply}
                            ),
                            case gleam_erlang_ffi:'receive'(Host_reply, 10000) of
                                {ok, Result} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        Result
                                    ),
                                    gleam@otp@actor:continue(State);

                                {error, _} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        {error,
                                            <<"Timeout waiting for host agent"/utf8>>}
                                    ),
                                    gleam@otp@actor:continue(State)
                            end;

                        {error, _} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Host agent not found for host "/utf8,
                                        Id/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                none ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM not assigned to a host"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM not found: "/utf8, (debug_error(Err))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 278).
-spec handle_stop_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_stop_vm(Vm_id, Reply_with, State) ->
    case blixard_khepri_store:get_vm(erlang:element(2, State), Vm_id) of
        {ok, Vm} ->
            case erlang:element(8, Vm) of
                {some, Id} ->
                    case gleam_stdlib:map_get(erlang:element(3, State), Id) of
                        {ok, Host_agent} ->
                            Host_reply = gleam@erlang@process:new_subject(),
                            gleam@erlang@process:send(
                                Host_agent,
                                {stop_vm, Vm_id, Host_reply}
                            ),
                            case gleam_erlang_ffi:'receive'(Host_reply, 10000) of
                                {ok, Result} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        Result
                                    ),
                                    gleam@otp@actor:continue(State);

                                {error, _} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        {error,
                                            <<"Timeout waiting for host agent"/utf8>>}
                                    ),
                                    gleam@otp@actor:continue(State)
                            end;

                        {error, _} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Host agent not found for host "/utf8,
                                        Id/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                none ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM not assigned to a host"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM not found: "/utf8, (debug_error(Err))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 338).
-spec handle_pause_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_pause_vm(Vm_id, Reply_with, State) ->
    case blixard_khepri_store:get_vm(erlang:element(2, State), Vm_id) of
        {ok, Vm} ->
            case erlang:element(8, Vm) of
                {some, Id} ->
                    case gleam_stdlib:map_get(erlang:element(3, State), Id) of
                        {ok, Host_agent} ->
                            Host_reply = gleam@erlang@process:new_subject(),
                            gleam@erlang@process:send(
                                Host_agent,
                                {pause_vm, Vm_id, Host_reply}
                            ),
                            case gleam_erlang_ffi:'receive'(Host_reply, 10000) of
                                {ok, Result} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        Result
                                    ),
                                    gleam@otp@actor:continue(State);

                                {error, _} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        {error,
                                            <<"Timeout waiting for host agent"/utf8>>}
                                    ),
                                    gleam@otp@actor:continue(State)
                            end;

                        {error, _} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Host agent not found for host "/utf8,
                                        Id/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                none ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM not assigned to a host"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM not found: "/utf8, (debug_error(Err))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 398).
-spec handle_resume_vm(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_resume_vm(Vm_id, Reply_with, State) ->
    case blixard_khepri_store:get_vm(erlang:element(2, State), Vm_id) of
        {ok, Vm} ->
            case erlang:element(8, Vm) of
                {some, Id} ->
                    case gleam_stdlib:map_get(erlang:element(3, State), Id) of
                        {ok, Host_agent} ->
                            Host_reply = gleam@erlang@process:new_subject(),
                            gleam@erlang@process:send(
                                Host_agent,
                                {resume_vm, Vm_id, Host_reply}
                            ),
                            case gleam_erlang_ffi:'receive'(Host_reply, 10000) of
                                {ok, Result} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        Result
                                    ),
                                    gleam@otp@actor:continue(State);

                                {error, _} ->
                                    gleam@erlang@process:send(
                                        Reply_with,
                                        {error,
                                            <<"Timeout waiting for host agent"/utf8>>}
                                    ),
                                    gleam@otp@actor:continue(State)
                            end;

                        {error, _} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Host agent not found for host "/utf8,
                                        Id/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                none ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error, <<"VM not assigned to a host"/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err} ->
            gleam@erlang@process:send(
                Reply_with,
                {error, <<"VM not found: "/utf8, (debug_error(Err))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 458).
-spec handle_register_host(
    blixard@domain@types:host(),
    gleam@erlang@process:subject({ok, blixard@domain@types:host()} |
        {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_register_host(Host, Reply_with, State) ->
    case validate_host(Host) of
        {ok, _} ->
            case blixard_khepri_store:put_host(erlang:element(2, State), Host) of
                {ok, _} ->
                    case blixard@host_agent@agent:start(
                        erlang:element(2, Host),
                        erlang:element(2, State)
                    ) of
                        {ok, Host_agent} ->
                            New_hosts = gleam@dict:insert(
                                erlang:element(3, State),
                                erlang:element(2, Host),
                                Host_agent
                            ),
                            New_state = begin
                                _record = State,
                                {state,
                                    erlang:element(2, _record),
                                    New_hosts,
                                    erlang:element(4, _record)}
                            end,
                            gleam@erlang@process:send(Reply_with, {ok, Host}),
                            gleam@otp@actor:continue(New_state);

                        {error, Reason} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Failed to start host agent: "/utf8,
                                        Reason/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                {error, Err} ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Failed to store host: "/utf8,
                                (debug_error(Err))/binary>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Reason@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Invalid host configuration: "/utf8, Reason@1/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 518).
-spec handle_deregister_host(
    binary(),
    gleam@erlang@process:subject({ok, nil} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_deregister_host(Host_id, Reply_with, State) ->
    case blixard_khepri_store:get_host(erlang:element(2, State), Host_id) of
        {ok, Host} ->
            case gleam@list:is_empty(erlang:element(9, Host)) of
                true ->
                    case blixard_khepri_store:delete_host(
                        erlang:element(2, State),
                        Host_id
                    ) of
                        {ok, _} ->
                            New_hosts = gleam@dict:delete(
                                erlang:element(3, State),
                                Host_id
                            ),
                            New_state = begin
                                _record = State,
                                {state,
                                    erlang:element(2, _record),
                                    New_hosts,
                                    erlang:element(4, _record)}
                            end,
                            gleam@erlang@process:send(Reply_with, {ok, nil}),
                            gleam@otp@actor:continue(New_state);

                        {error, Err} ->
                            gleam@erlang@process:send(
                                Reply_with,
                                {error,
                                    <<"Failed to delete host: "/utf8,
                                        (debug_error(Err))/binary>>}
                            ),
                            gleam@otp@actor:continue(State)
                    end;

                false ->
                    gleam@erlang@process:send(
                        Reply_with,
                        {error,
                            <<"Host has running VMs. Stop them first."/utf8>>}
                    ),
                    gleam@otp@actor:continue(State)
            end;

        {error, Err@1} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Host not found: "/utf8, (debug_error(Err@1))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 568).
-spec handle_get_system_status(
    gleam@erlang@process:subject({ok, system_status()} | {error, binary()}),
    state()
) -> gleam@otp@actor:next(command(), state()).
handle_get_system_status(Reply_with, State) ->
    Vms_result = blixard_khepri_store:list_vms(erlang:element(2, State)),
    Hosts_result = blixard_khepri_store:list_hosts(erlang:element(2, State)),
    case {Vms_result, Hosts_result} of
        {{ok, Vms}, {ok, Hosts}} ->
            Grouped_vms = begin
                _pipe = Vms,
                gleam@list:group(_pipe, fun(Vm) -> erlang:element(7, Vm) end)
            end,
            State_counts = begin
                _pipe@1 = Grouped_vms,
                _pipe@2 = maps:to_list(_pipe@1),
                gleam@list:map(
                    _pipe@2,
                    fun(Tuple) ->
                        {erlang:element(1, Tuple),
                            erlang:length(erlang:element(2, Tuple))}
                    end
                )
            end,
            Vms_by_state = maps:from_list(State_counts),
            Status = {system_status,
                erlang:length(Vms),
                erlang:length(Hosts),
                Vms_by_state},
            gleam@erlang@process:send(Reply_with, {ok, Status}),
            gleam@otp@actor:continue(State);

        {{error, Err}, _} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Failed to retrieve VMs: "/utf8,
                        (debug_error(Err))/binary>>}
            ),
            gleam@otp@actor:continue(State);

        {_, {error, Err@1}} ->
            gleam@erlang@process:send(
                Reply_with,
                {error,
                    <<"Failed to retrieve hosts: "/utf8,
                        (debug_error(Err@1))/binary>>}
            ),
            gleam@otp@actor:continue(State)
    end.

-file("src/blixard/orchestrator/core.gleam", 84).
-spec handle_message(command(), state()) -> gleam@otp@actor:next(command(), state()).
handle_message(Message, State) ->
    case Message of
        {create_vm, Vm, Reply_with} ->
            handle_create_vm(Vm, Reply_with, State);

        {delete_vm, Vm_id, Reply_with@1} ->
            handle_delete_vm(Vm_id, Reply_with@1, State);

        {start_vm, Vm_id@1, Reply_with@2} ->
            handle_start_vm(Vm_id@1, Reply_with@2, State);

        {stop_vm, Vm_id@2, Reply_with@3} ->
            handle_stop_vm(Vm_id@2, Reply_with@3, State);

        {pause_vm, Vm_id@3, Reply_with@4} ->
            handle_pause_vm(Vm_id@3, Reply_with@4, State);

        {resume_vm, Vm_id@4, Reply_with@5} ->
            handle_resume_vm(Vm_id@4, Reply_with@5, State);

        {register_host, Host, Reply_with@6} ->
            handle_register_host(Host, Reply_with@6, State);

        {deregister_host, Host_id, Reply_with@7} ->
            handle_deregister_host(Host_id, Reply_with@7, State);

        {get_system_status, Reply_with@8} ->
            handle_get_system_status(Reply_with@8, State)
    end.

-file("src/blixard/orchestrator/core.gleam", 63).
-spec start(blixard@storage@khepri_store:khepri()) -> {ok,
        gleam@erlang@process:subject(command())} |
    {error, binary()}.
start(Store) ->
    Initial_state = {state, Store, maps:new(), most_available},
    case gleam@otp@actor:start(Initial_state, fun handle_message/2) of
        {ok, Actor} ->
            {ok, Actor};

        {error, Err} ->
            {error,
                <<"Failed to start actor: "/utf8,
                    (debug_actor_error(Err))/binary>>}
    end.
