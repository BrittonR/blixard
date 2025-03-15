-module(blixard@scheduler@scheduler).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([schedule_vm/3]).
-export_type([scheduling_strategy/0, scheduling_result/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/scheduler/scheduler.gleam\n").

-type scheduling_strategy() :: most_available |
    bin_packing |
    {specific_host, binary()} |
    {tag_affinity, list(binary())}.

-type scheduling_result() :: {scheduled, binary(), binary()} |
    {no_suitable_host, binary()} |
    {scheduling_error, binary()}.

-file("src/blixard/scheduler/scheduler.gleam", 36).
?DOC(" Check if a host has enough resources for a VM\n").
-spec has_enough_resources(
    blixard@domain@types:host(),
    blixard@domain@types:micro_vm()
) -> boolean().
has_enough_resources(Host, Vm) ->
    Host_resources = erlang:element(7, Host),
    Vm_resources = erlang:element(6, Vm),
    ((erlang:element(2, Host_resources) >= erlang:element(2, Vm_resources))
    andalso (erlang:element(3, Host_resources) >= erlang:element(
        3,
        Vm_resources
    )))
    andalso (erlang:element(4, Host_resources) >= erlang:element(
        4,
        Vm_resources
    )).

-file("src/blixard/scheduler/scheduler.gleam", 46).
?DOC(" Calculate remaining resources after placing a VM\n").
-spec calculate_remaining_resources(
    blixard@domain@types:host(),
    blixard@domain@types:micro_vm()
) -> blixard@domain@types:resources().
calculate_remaining_resources(Host, Vm) ->
    Host_resources = erlang:element(7, Host),
    Vm_resources = erlang:element(6, Vm),
    {resources,
        erlang:element(2, Host_resources) - erlang:element(2, Vm_resources),
        erlang:element(3, Host_resources) - erlang:element(3, Vm_resources),
        erlang:element(4, Host_resources) - erlang:element(4, Vm_resources)}.

-file("src/blixard/scheduler/scheduler.gleam", 58).
?DOC(" Find a suitable host using the MostAvailable strategy\n").
-spec find_most_available_host(
    list(blixard@domain@types:host()),
    blixard@domain@types:micro_vm()
) -> {ok, blixard@domain@types:host()} | {error, binary()}.
find_most_available_host(Hosts, Vm) ->
    _pipe = Hosts,
    _pipe@1 = gleam@list:filter(
        _pipe,
        fun(Host) ->
            erlang:element(10, Host) andalso has_enough_resources(Host, Vm)
        end
    ),
    _pipe@2 = gleam@list:sort(
        _pipe@1,
        fun(A, B) ->
            A_total = ((erlang:element(2, erlang:element(7, A)) * 100) + (erlang:element(
                3,
                erlang:element(7, A)
            )
            div 1024))
            + erlang:element(4, erlang:element(7, A)),
            B_total = ((erlang:element(2, erlang:element(7, B)) * 100) + (erlang:element(
                3,
                erlang:element(7, B)
            )
            div 1024))
            + erlang:element(4, erlang:element(7, B)),
            gleam@int:compare(B_total, A_total)
        end
    ),
    _pipe@3 = gleam@list:first(_pipe@2),
    gleam@result:replace_error(
        _pipe@3,
        <<"No suitable host found with enough resources"/utf8>>
    ).

-file("src/blixard/scheduler/scheduler.gleam", 88).
?DOC(" Find a suitable host using the BinPacking strategy\n").
-spec find_bin_packing_host(
    list(blixard@domain@types:host()),
    blixard@domain@types:micro_vm()
) -> {ok, blixard@domain@types:host()} | {error, binary()}.
find_bin_packing_host(Hosts, Vm) ->
    _pipe = Hosts,
    _pipe@1 = gleam@list:filter(
        _pipe,
        fun(Host) ->
            erlang:element(10, Host) andalso has_enough_resources(Host, Vm)
        end
    ),
    _pipe@2 = gleam@list:sort(
        _pipe@1,
        fun(A, B) ->
            A_remaining = calculate_remaining_resources(A, Vm),
            B_remaining = calculate_remaining_resources(B, Vm),
            A_total = ((erlang:element(2, A_remaining) * 100) + (erlang:element(
                3,
                A_remaining
            )
            div 1024))
            + erlang:element(4, A_remaining),
            B_total = ((erlang:element(2, B_remaining) * 100) + (erlang:element(
                3,
                B_remaining
            )
            div 1024))
            + erlang:element(4, B_remaining),
            gleam@int:compare(A_total, B_total)
        end
    ),
    _pipe@3 = gleam@list:first(_pipe@2),
    gleam@result:replace_error(
        _pipe@3,
        <<"No suitable host found with enough resources"/utf8>>
    ).

-file("src/blixard/scheduler/scheduler.gleam", 118).
?DOC(" Find a host with specific tags\n").
-spec find_host_with_tags(
    list(blixard@domain@types:host()),
    blixard@domain@types:micro_vm(),
    list(binary())
) -> {ok, blixard@domain@types:host()} | {error, binary()}.
find_host_with_tags(Hosts, Vm, Required_tags) ->
    Hosts_with_tags = gleam@list:filter(
        Hosts,
        fun(Host) ->
            gleam@list:all(
                Required_tags,
                fun(Tag) ->
                    gleam@list:contains(erlang:element(11, Host), Tag)
                end
            )
        end
    ),
    case Hosts_with_tags of
        [] ->
            {error, <<"No host found with the required tags"/utf8>>};

        _ ->
            find_most_available_host(Hosts_with_tags, Vm)
    end.

-file("src/blixard/scheduler/scheduler.gleam", 137).
?DOC(" Find a specific host by ID\n").
-spec find_specific_host(
    list(blixard@domain@types:host()),
    blixard@domain@types:micro_vm(),
    binary()
) -> {ok, blixard@domain@types:host()} | {error, binary()}.
find_specific_host(Hosts, Vm, Host_id) ->
    _pipe = Hosts,
    _pipe@1 = gleam@list:find(
        _pipe,
        fun(Host) -> erlang:element(2, Host) =:= Host_id end
    ),
    _pipe@2 = gleam@result:replace_error(
        _pipe@1,
        <<"Host not found with ID: "/utf8, Host_id/binary>>
    ),
    gleam@result:then(
        _pipe@2,
        fun(Host@1) ->
            case erlang:element(10, Host@1) andalso has_enough_resources(
                Host@1,
                Vm
            ) of
                true ->
                    {ok, Host@1};

                false ->
                    {error, <<"Specified host cannot accommodate the VM"/utf8>>}
            end
        end
    ).

-file("src/blixard/scheduler/scheduler.gleam", 208).
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

-file("src/blixard/scheduler/scheduler.gleam", 154).
?DOC(" Schedule a VM on a suitable host\n").
-spec schedule_vm(
    blixard@storage@khepri_store:khepri(),
    binary(),
    scheduling_strategy()
) -> scheduling_result().
schedule_vm(Store, Vm_id, Strategy) ->
    Vm_result = blixard_khepri_store:get_vm(Store, Vm_id),
    Hosts_result = blixard_khepri_store:list_hosts(Store),
    case {Vm_result, Hosts_result} of
        {{ok, Vm}, {ok, Hosts}} ->
            Host_result = case Strategy of
                most_available ->
                    find_most_available_host(Hosts, Vm);

                bin_packing ->
                    find_bin_packing_host(Hosts, Vm);

                {specific_host, Host_id} ->
                    find_specific_host(Hosts, Vm, Host_id);

                {tag_affinity, Tags} ->
                    find_host_with_tags(Hosts, Vm, Tags)
            end,
            case Host_result of
                {ok, Host} ->
                    case blixard_khepri_store:assign_vm_to_host(
                        Store,
                        Vm_id,
                        erlang:element(2, Host)
                    ) of
                        {ok, _} ->
                            case blixard_khepri_store:update_vm_state(
                                Store,
                                Vm_id,
                                scheduling
                            ) of
                                {ok, _} ->
                                    {scheduled, Vm_id, erlang:element(2, Host)};

                                {error, Err} ->
                                    {scheduling_error,
                                        <<"Failed to update VM state: "/utf8,
                                            (debug_error(Err))/binary>>}
                            end;

                        {error, Err@1} ->
                            {scheduling_error,
                                <<"Failed to assign VM to host: "/utf8,
                                    (debug_error(Err@1))/binary>>}
                    end;

                {error, Reason} ->
                    {no_suitable_host, Reason}
            end;

        {{error, Err@2}, _} ->
            {scheduling_error,
                <<"Failed to retrieve VM: "/utf8, (debug_error(Err@2))/binary>>};

        {_, {error, Err@3}} ->
            {scheduling_error,
                <<"Failed to retrieve hosts: "/utf8,
                    (debug_error(Err@3))/binary>>}
    end.
