-module(blixard).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([main/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard.gleam\n").

-file("src/blixard.gleam", 79).
?DOC(" Helper function to debug KhepriError with safer handling\n").
-spec safe_debug_error(blixard@storage@khepri_store:khepri_error()) -> binary().
safe_debug_error(Error) ->
    case Error of
        {connection_error, Msg} ->
            <<"Connection error: "/utf8, (gleam@string:inspect(Msg))/binary>>;

        {consensus_error, Msg@1} ->
            <<"Consensus error: "/utf8, (gleam@string:inspect(Msg@1))/binary>>;

        {storage_error, Msg@2} ->
            <<"Storage error: "/utf8, (gleam@string:inspect(Msg@2))/binary>>;

        not_found ->
            <<"Resource not found"/utf8>>;

        {invalid_data, Msg@3} ->
            <<"Invalid data: "/utf8, (gleam@string:inspect(Msg@3))/binary>>;

        _ ->
            <<"Unknown error: "/utf8, (gleam@string:inspect(Error))/binary>>
    end.

-file("src/blixard.gleam", 46).
-spec run_normal() -> nil.
run_normal() ->
    gleam_stdlib:println(
        <<"Starting Blixard - NixOS microVM orchestrator"/utf8>>
    ),
    gleam_stdlib:println(<<"Initializing Khepri store..."/utf8>>),
    Store_result = blixard_khepri_store:start(
        [<<"blixard@127.0.0.1"/utf8>>],
        <<"blixard_cluster"/utf8>>
    ),
    case Store_result of
        {ok, Store} ->
            gleam_stdlib:println(
                <<"Khepri store initialized successfully!"/utf8>>
            ),
            gleam_stdlib:println(
                <<"Blixard initialized! This is a placeholder implementation."/utf8>>
            ),
            gleam_stdlib:println(
                <<"In a real implementation, we would start the orchestrator and API server."/utf8>>
            ),
            gleam_stdlib:println(<<"\nOptions:"/utf8>>),
            gleam_stdlib:println(
                <<"  --test-khepri : Run Khepri store tests"/utf8>>
            ),
            gleam_stdlib:println(
                <<"  --debug-ffi   : Run FFI debugging tests"/utf8>>
            ),
            gleam_erlang_ffi:sleep_forever();

        {error, Err} ->
            gleam_stdlib:println(
                <<"Failed to initialize Khepri store: "/utf8,
                    (safe_debug_error(Err))/binary>>
            )
    end.

-file("src/blixard.gleam", 23).
-spec main() -> nil.
main() ->
    Args = gleam@erlang:start_arguments(),
    case Args of
        [<<"--debug-ffi"/utf8>>] ->
            blixard@debug_test:main();

        [<<"--test-khepri"/utf8>>] ->
            blixard@test_khepri:main();

        _ ->
            run_normal()
    end.
