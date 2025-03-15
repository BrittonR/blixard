-module(blixard@debug_test).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([print_vm/1, create_module/0, main/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/debug_test.gleam\n").

-file("src/blixard/debug_test.gleam", 17).
-spec print_vm(blixard@domain@types:micro_vm()) -> nil.
print_vm(Vm) ->
    blixard_debug:print_vm(Vm).

-file("src/blixard/debug_test.gleam", 21).
-spec create_module() -> nil.
create_module() ->
    blixard_debug:create_module().

-file("src/blixard/debug_test.gleam", 23).
-spec main() -> nil.
main() ->
    gleam_stdlib:println(<<"Debugging Gleam-Erlang FFI"/utf8>>),
    gleam_stdlib:println(<<"=========================="/utf8>>),
    blixard_debug:create_module(),
    gleam_stdlib:println(<<"\nCreating test microVM..."/utf8>>),
    Vm = {micro_vm,
        <<"test-vm-1"/utf8>>,
        <<"Test VM"/utf8>>,
        {some, <<"Test description"/utf8>>},
        persistent,
        {resources, 2, 2048, 20},
        pending,
        none,
        [],
        [],
        {tailscale_config,
            true,
            none,
            <<"test-vm"/utf8>>,
            [<<"test"/utf8>>],
            true},
        {nixos_config, <<"/test/path"/utf8>>, maps:new(), none},
        maps:new(),
        <<"now"/utf8>>,
        <<"now"/utf8>>},
    gleam_stdlib:println(<<"Printing VM structure via FFI..."/utf8>>),
    blixard_debug:print_vm(Vm),
    gleam_stdlib:println(<<"\nInitializing Khepri store..."/utf8>>),
    Store_result = blixard_khepri_store:start(
        [<<"blixard@127.0.0.1"/utf8>>],
        <<"blixard_test_cluster"/utf8>>
    ),
    case Store_result of
        {ok, Store} ->
            gleam_stdlib:println(
                <<"Khepri store initialized successfully!"/utf8>>
            ),
            gleam_stdlib:println(<<"\nStoring VM in Khepri..."/utf8>>),
            case blixard_khepri_store:put_vm(Store, Vm) of
                {ok, _} ->
                    gleam_stdlib:println(<<"VM stored successfully!"/utf8>>);

                {error, Err} ->
                    gleam_stdlib:println(
                        <<"Failed to store VM: "/utf8,
                            (gleam@string:inspect(Err))/binary>>
                    )
            end;

        {error, Err@1} ->
            gleam_stdlib:println(
                <<"Failed to initialize Khepri store: "/utf8,
                    (gleam@string:inspect(Err@1))/binary>>
            )
    end.
