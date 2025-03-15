-module(shellout).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([display/1, color/1, background/1, arguments/0, command/4, exit/1, which/1, style/3]).
-export_type([style/0, style_acc/0, command_opt/0]).

-type style() :: {name, binary()} | {rgb, list(binary())}.

-type style_acc() :: {style_acc, list(style()), integer()}.

-type command_opt() :: let_be_stderr | let_be_stdout | overlapped_stdio.

-spec display(list(binary())) -> gleam@dict:dict(binary(), list(binary())).
display(Values) ->
    maps:from_list([{<<"display"/utf8>>, Values}]).

-spec color(list(binary())) -> gleam@dict:dict(binary(), list(binary())).
color(Values) ->
    maps:from_list([{<<"color"/utf8>>, Values}]).

-spec background(list(binary())) -> gleam@dict:dict(binary(), list(binary())).
background(Values) ->
    maps:from_list([{<<"background"/utf8>>, Values}]).

-spec escape(binary(), binary()) -> binary().
escape(Code, String) ->
    <<<<<<<<"\x{1b}["/utf8, Code/binary>>/binary, "m"/utf8>>/binary,
            String/binary>>/binary,
        "\x{1b}[0m\x{1b}[K"/utf8>>.

-spec arguments() -> list(binary()).
arguments() ->
    shellout_ffi:start_arguments().

-spec command(binary(), list(binary()), binary(), list(command_opt())) -> {ok,
        binary()} |
    {error, {integer(), binary()}}.
command(Executable, Arguments, Directory, Options) ->
    _pipe = Options,
    _pipe@1 = gleam@list:map(_pipe, fun(Opt) -> {Opt, true} end),
    _pipe@2 = maps:from_list(_pipe@1),
    shellout_ffi:os_command(Executable, Arguments, Directory, _pipe@2).

-spec exit(integer()) -> nil.
exit(Status) ->
    shellout_ffi:os_exit(Status).

-spec which(binary()) -> {ok, binary()} | {error, binary()}.
which(Executable) ->
    shellout_ffi:os_which(Executable).

-spec do_style(list({binary(), list(binary())}), list(binary()), binary()) -> list(binary()).
do_style(Lookup, Strings, Flag) ->
    Lookup@1 = begin
        _pipe@2 = case Flag of
            <<"display"/utf8>> ->
                maps:from_list(
                    [{<<"reset"/utf8>>, [<<"0"/utf8>>]},
                        {<<"bold"/utf8>>, [<<"1"/utf8>>]},
                        {<<"dim"/utf8>>, [<<"2"/utf8>>]},
                        {<<"italic"/utf8>>, [<<"3"/utf8>>]},
                        {<<"underline"/utf8>>, [<<"4"/utf8>>]},
                        {<<"blink"/utf8>>, [<<"5"/utf8>>]},
                        {<<"fastblink"/utf8>>, [<<"6"/utf8>>]},
                        {<<"reverse"/utf8>>, [<<"7"/utf8>>]},
                        {<<"hide"/utf8>>, [<<"8"/utf8>>]},
                        {<<"strike"/utf8>>, [<<"9"/utf8>>]},
                        {<<"normal"/utf8>>, [<<"22"/utf8>>]},
                        {<<"noitalic"/utf8>>, [<<"23"/utf8>>]},
                        {<<"nounderline"/utf8>>, [<<"24"/utf8>>]},
                        {<<"noblink"/utf8>>, [<<"25"/utf8>>]},
                        {<<"noreverse"/utf8>>, [<<"27"/utf8>>]},
                        {<<"nohide"/utf8>>, [<<"28"/utf8>>]},
                        {<<"nostrike"/utf8>>, [<<"29"/utf8>>]}]
                );

            <<"color"/utf8>> ->
                maps:from_list(
                    [{<<"black"/utf8>>, [<<"30"/utf8>>]},
                        {<<"red"/utf8>>, [<<"31"/utf8>>]},
                        {<<"green"/utf8>>, [<<"32"/utf8>>]},
                        {<<"yellow"/utf8>>, [<<"33"/utf8>>]},
                        {<<"blue"/utf8>>, [<<"34"/utf8>>]},
                        {<<"magenta"/utf8>>, [<<"35"/utf8>>]},
                        {<<"cyan"/utf8>>, [<<"36"/utf8>>]},
                        {<<"white"/utf8>>, [<<"37"/utf8>>]},
                        {<<"default"/utf8>>, [<<"39"/utf8>>]},
                        {<<"brightblack"/utf8>>, [<<"90"/utf8>>]},
                        {<<"brightred"/utf8>>, [<<"91"/utf8>>]},
                        {<<"brightgreen"/utf8>>, [<<"92"/utf8>>]},
                        {<<"brightyellow"/utf8>>, [<<"93"/utf8>>]},
                        {<<"brightblue"/utf8>>, [<<"94"/utf8>>]},
                        {<<"brightmagenta"/utf8>>, [<<"95"/utf8>>]},
                        {<<"brightcyan"/utf8>>, [<<"96"/utf8>>]},
                        {<<"brightwhite"/utf8>>, [<<"97"/utf8>>]}]
                );

            <<"background"/utf8>> ->
                _pipe = [{<<"black"/utf8>>, [<<"30"/utf8>>]},
                    {<<"red"/utf8>>, [<<"31"/utf8>>]},
                    {<<"green"/utf8>>, [<<"32"/utf8>>]},
                    {<<"yellow"/utf8>>, [<<"33"/utf8>>]},
                    {<<"blue"/utf8>>, [<<"34"/utf8>>]},
                    {<<"magenta"/utf8>>, [<<"35"/utf8>>]},
                    {<<"cyan"/utf8>>, [<<"36"/utf8>>]},
                    {<<"white"/utf8>>, [<<"37"/utf8>>]},
                    {<<"default"/utf8>>, [<<"39"/utf8>>]},
                    {<<"brightblack"/utf8>>, [<<"90"/utf8>>]},
                    {<<"brightred"/utf8>>, [<<"91"/utf8>>]},
                    {<<"brightgreen"/utf8>>, [<<"92"/utf8>>]},
                    {<<"brightyellow"/utf8>>, [<<"93"/utf8>>]},
                    {<<"brightblue"/utf8>>, [<<"94"/utf8>>]},
                    {<<"brightmagenta"/utf8>>, [<<"95"/utf8>>]},
                    {<<"brightcyan"/utf8>>, [<<"96"/utf8>>]},
                    {<<"brightwhite"/utf8>>, [<<"97"/utf8>>]}],
                _pipe@1 = maps:from_list(_pipe),
                gleam@dict:map_values(
                    _pipe@1,
                    fun(_, Code) ->
                        [Code@1] = case Code of
                            [_] -> Code;
                            _assert_fail ->
                                erlang:error(#{gleam_error => let_assert,
                                            message => <<"Assertion pattern match failed"/utf8>>,
                                            value => _assert_fail,
                                            module => <<"shellout"/utf8>>,
                                            function => <<"do_style"/utf8>>,
                                            line => 235})
                        end,
                        _assert_subject = gleam@int:parse(Code@1),
                        {ok, Code@2} = case _assert_subject of
                            {ok, _} -> _assert_subject;
                            _assert_fail@1 ->
                                erlang:error(#{gleam_error => let_assert,
                                            message => <<"Assertion pattern match failed"/utf8>>,
                                            value => _assert_fail@1,
                                            module => <<"shellout"/utf8>>,
                                            function => <<"do_style"/utf8>>,
                                            line => 236})
                        end,
                        [gleam@int:to_string(Code@2 + 10)]
                    end
                );

            _ ->
                erlang:error(#{gleam_error => panic,
                        message => <<"invalid lookup flag"/utf8>>,
                        module => <<"shellout"/utf8>>,
                        function => <<"do_style"/utf8>>,
                        line => 239})
        end,
        gleam@dict:merge(_pipe@2, maps:from_list(Lookup))
    end,
    Acc = {style_acc, [], 0},
    Acc@2 = begin
        _pipe@3 = Strings,
        gleam@list:fold(
            _pipe@3,
            Acc,
            fun(Acc@1, Item) -> case gleam@int:parse(Item) of
                    {ok, Int} ->
                        Item@1 = begin
                            _pipe@4 = Int,
                            _pipe@5 = gleam@int:clamp(_pipe@4, 0, 255),
                            gleam@int:to_string(_pipe@5)
                        end,
                        Rgb_counter = erlang:element(3, Acc@1),
                        case Rgb_counter < 3 of
                            true when Rgb_counter > 0 ->
                                _assert_subject@1 = erlang:element(2, Acc@1),
                                [{rgb, Values} | Styles] = case _assert_subject@1 of
                                    [{rgb, _} | _] -> _assert_subject@1;
                                    _assert_fail@2 ->
                                        erlang:error(
                                                #{gleam_error => let_assert,
                                                    message => <<"Assertion pattern match failed"/utf8>>,
                                                    value => _assert_fail@2,
                                                    module => <<"shellout"/utf8>>,
                                                    function => <<"do_style"/utf8>>,
                                                    line => 256}
                                            )
                                end,
                                {style_acc,
                                    [{rgb, [Item@1 | Values]} | Styles],
                                    Rgb_counter + 1};

                            _ ->
                                {style_acc,
                                    [{rgb, [Item@1]} | erlang:element(2, Acc@1)],
                                    1}
                        end;

                    _ ->
                        {style_acc,
                            [{name, Item} | erlang:element(2, Acc@1)],
                            0}
                end end
        )
    end,
    Prepare_rgb = fun(Strings@1) ->
        New_strings = begin
            _pipe@6 = <<"0"/utf8>>,
            _pipe@7 = gleam@list:repeat(
                _pipe@6,
                3 - gleam@list:length(Strings@1)
            ),
            gleam@list:append(Strings@1, _pipe@7)
        end,
        Code@3 = case Flag of
            <<"color"/utf8>> ->
                <<"38"/utf8>>;

            _ ->
                <<"48"/utf8>>
        end,
        [Code@3, <<"2"/utf8>> | New_strings]
    end,
    _pipe@8 = erlang:element(2, Acc@2),
    _pipe@9 = gleam@list:reverse(_pipe@8),
    _pipe@15 = gleam@list:filter_map(_pipe@9, fun(Style) -> case Style of
                {name, String} ->
                    _pipe@10 = Lookup@1,
                    _pipe@11 = gleam@dict:get(_pipe@10, String),
                    gleam@result:map(
                        _pipe@11,
                        fun(Strings@2) ->
                            case gleam@list:length(Strings@2) > 1 of
                                false ->
                                    Strings@2;

                                true ->
                                    Prepare_rgb(Strings@2)
                            end
                        end
                    );

                {rgb, Strings@3} ->
                    _pipe@12 = Strings@3,
                    _pipe@13 = gleam@list:reverse(_pipe@12),
                    _pipe@14 = Prepare_rgb(_pipe@13),
                    {ok, _pipe@14}
            end end),
    gleam@list:flatten(_pipe@15).

-spec style(
    binary(),
    gleam@dict:dict(binary(), list(binary())),
    list({list(binary()), list({binary(), list(binary())})})
) -> binary().
style(String, Flags, Lookups) ->
    _pipe = [<<"display"/utf8>>, <<"color"/utf8>>, <<"background"/utf8>>],
    _pipe@7 = gleam@list:map(
        _pipe,
        fun(Flag) ->
            gleam@result:'try'(
                gleam@dict:get(Flags, Flag),
                fun(Strings) -> _pipe@1 = Lookups,
                    _pipe@4 = gleam@list:filter_map(
                        _pipe@1,
                        fun(Item) ->
                            {Keys, Lookup} = Item,
                            _pipe@2 = Keys,
                            _pipe@3 = gleam@list:find(
                                _pipe@2,
                                fun(Key) -> Key =:= Flag end
                            ),
                            gleam@result:map(_pipe@3, fun(_) -> Lookup end)
                        end
                    ),
                    _pipe@5 = gleam@list:flatten(_pipe@4),
                    _pipe@6 = do_style(_pipe@5, Strings, Flag),
                    {ok, _pipe@6} end
            )
        end
    ),
    _pipe@8 = gleam@result:values(_pipe@7),
    _pipe@9 = gleam@list:flatten(_pipe@8),
    _pipe@10 = gleam@string:join(_pipe@9, <<";"/utf8>>),
    escape(_pipe@10, String).
