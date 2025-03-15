-module(gleam@otp@task).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([async/1, try_await/2, await/2, pid/1, await_forever/1, try_await2/3, try_await3/4, try_await4/5, try_await_all/2]).
-export_type([task/1, await_error/0, message2/2, message3/3, message4/4, message/1]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(
    " A task is a kind of process that computes a value and then sends the result back\n"
    " to its parent. Commonly multiple tasks are used to compute multiple things at\n"
    " once.\n"
    "\n"
    " If you do not care to receive a result back at the end then you should not\n"
    " use this module, `actor` or `process` are likely more suitable.\n"
    "\n"
    " ```gleam\n"
    " let task = task.async(fn() { do_some_work() })\n"
    " let value = do_some_other_work()\n"
    " value + task.await(task, 100)\n"
    " ```\n"
    "\n"
    " Tasks spawned with async can be awaited on by their caller process (and\n"
    " only their caller) as shown in the example above. They are implemented by\n"
    " spawning a process that sends a message to the caller once the given\n"
    " computation is performed.\n"
    "\n"
    " There are some important things to consider when using tasks:\n"
    "\n"
    " 1. If you are using async tasks, you must await a reply as they are always\n"
    "    sent.\n"
    "\n"
    " 2. Tasks link the caller and the spawned process. This means that,\n"
    "    if the caller crashes, the task will crash too and vice-versa. This is\n"
    "    on purpose: if the process meant to receive the result no longer\n"
    "    exists, there is no purpose in completing the computation.\n"
    "\n"
    " 3. A task's callback function must complete by returning or panicking.\n"
    "    It must not `exit` with the reason \"normal\".\n"
    "\n"
    " This module is inspired by Elixir's [Task module][1].\n"
    "\n"
    " [1]: https://hexdocs.pm/elixir/master/Task.html\n"
    "\n"
).

-opaque task(GSS) :: {task,
        gleam@erlang@process:pid_(),
        gleam@erlang@process:pid_(),
        gleam@erlang@process:subject(GSS)}.

-type await_error() :: timeout | {exit, gleam@dynamic:dynamic_()}.

-type message2(GST, GSU) :: {m2_from_subject1, GST} |
    {m2_from_subject2, GSU} |
    m2_timeout.

-type message3(GSV, GSW, GSX) :: {m3_from_subject1, GSV} |
    {m3_from_subject2, GSW} |
    {m3_from_subject3, GSX} |
    m3_timeout.

-type message4(GSY, GSZ, GTA, GTB) :: {m4_from_subject1, GSY} |
    {m4_from_subject2, GSZ} |
    {m4_from_subject3, GTA} |
    {m4_from_subject4, GTB} |
    m4_timeout.

-type message(GTC) :: {message, integer(), GTC} | message_timeout.

-file("src/gleam/otp/task.gleam", 55).
?DOC(
    " Spawn a task process that calls a given function in order to perform some\n"
    " work. The result of this function is sent back to the parent and can be\n"
    " received using the `await` function.\n"
    "\n"
    " See the top level module documentation for more information on async/await.\n"
).
-spec async(fun(() -> GTD)) -> task(GTD).
async(Work) ->
    Owner = erlang:self(),
    Subject = gleam@erlang@process:new_subject(),
    Pid = gleam@erlang@process:start(
        fun() -> gleam@erlang@process:send(Subject, Work()) end,
        true
    ),
    {task, Owner, Pid, Subject}.

-file("src/gleam/otp/task.gleam", 70).
-spec assert_owner(task(any())) -> nil.
assert_owner(Task) ->
    Self = erlang:self(),
    case erlang:element(2, Task) =:= Self of
        true ->
            nil;

        false ->
            gleam@erlang@process:send_abnormal_exit(
                Self,
                <<"awaited on a task that does not belong to this process"/utf8>>
            )
    end.

-file("src/gleam/otp/task.gleam", 88).
?DOC(
    " Wait for the value computed by a task.\n"
    "\n"
    " If the a value is not received before the timeout has elapsed then an error\n"
    " is returned.\n"
).
-spec try_await(task(GTH), integer()) -> {ok, GTH} | {error, await_error()}.
try_await(Task, Timeout) ->
    assert_owner(Task),
    Selector = begin
        _pipe = gleam_erlang_ffi:new_selector(),
        gleam@erlang@process:selecting(
            _pipe,
            erlang:element(4, Task),
            fun gleam@function:identity/1
        )
    end,
    case gleam_erlang_ffi:select(Selector, Timeout) of
        {ok, X} ->
            {ok, X};

        {error, nil} ->
            {error, timeout}
    end.

-file("src/gleam/otp/task.gleam", 107).
?DOC(
    " Wait for the value computed by a task.\n"
    "\n"
    " If the a value is not received before the timeout has elapsed or if the\n"
    " task process crashes then this function crashes.\n"
).
-spec await(task(GTL), integer()) -> GTL.
await(Task, Timeout) ->
    _assert_subject = try_await(Task, Timeout),
    {ok, Value} = case _assert_subject of
        {ok, _} -> _assert_subject;
        _assert_fail ->
            erlang:error(#{gleam_error => let_assert,
                        message => <<"Pattern match failed, no pattern matched the value."/utf8>>,
                        value => _assert_fail,
                        module => <<"gleam/otp/task"/utf8>>,
                        function => <<"await"/utf8>>,
                        line => 108})
    end,
    Value.

-file("src/gleam/otp/task.gleam", 114).
?DOC(" Get the `Pid` for a task.\n").
-spec pid(task(any())) -> gleam@erlang@process:pid_().
pid(Task) ->
    erlang:element(3, Task).

-file("src/gleam/otp/task.gleam", 125).
?DOC(
    " Wait endlessly for the value computed by a task.\n"
    "\n"
    " Be Careful! Like `try_await_forever`, this function does not return until\n"
    " there is a value to receive.\n"
    "\n"
    " If the task process crashes then this function crashes.\n"
).
-spec await_forever(task(GTP)) -> GTP.
await_forever(Task) ->
    assert_owner(Task),
    Selector = begin
        _pipe = gleam_erlang_ffi:new_selector(),
        gleam@erlang@process:selecting(
            _pipe,
            erlang:element(4, Task),
            fun gleam@function:identity/1
        )
    end,
    gleam_erlang_ffi:select(Selector).

-file("src/gleam/otp/task.gleam", 162).
-spec try_await2_loop(
    gleam@erlang@process:selector(message2(GTZ, GUA)),
    gleam@option:option({ok, GTZ} | {error, await_error()}),
    gleam@option:option({ok, GUA} | {error, await_error()}),
    gleam@erlang@process:timer()
) -> {{ok, GTZ} | {error, await_error()}, {ok, GUA} | {error, await_error()}}.
try_await2_loop(Selector, T1, T2, Timer) ->
    case {T1, T2} of
        {{some, T1@1}, {some, T2@1}} ->
            gleam@erlang@process:cancel_timer(Timer),
            {T1@1, T2@1};

        {_, _} ->
            case gleam_erlang_ffi:select(Selector) of
                {m2_from_subject1, X} ->
                    T1@2 = {some, {ok, X}},
                    try_await2_loop(Selector, T1@2, T2, Timer);

                {m2_from_subject2, X@1} ->
                    T2@2 = {some, {ok, X@1}},
                    try_await2_loop(Selector, T1, T2@2, Timer);

                m2_timeout ->
                    {gleam@option:unwrap(T1, {error, timeout}),
                        gleam@option:unwrap(T2, {error, timeout})}
            end
    end.

-file("src/gleam/otp/task.gleam", 144).
?DOC(
    " Wait for the values computed by multiple tasks.\n"
    "\n"
    " For each task, if the a value is not received before the timeout has\n"
    " elapsed then an error is returned.\n"
).
-spec try_await2(task(GTR), task(GTT), integer()) -> {{ok, GTR} |
        {error, await_error()},
    {ok, GTT} | {error, await_error()}}.
try_await2(Task1, Task2, Timeout) ->
    assert_owner(Task1),
    assert_owner(Task2),
    Timeout_subject = gleam@erlang@process:new_subject(),
    Timer = gleam@erlang@process:send_after(
        Timeout_subject,
        Timeout,
        m2_timeout
    ),
    _pipe = gleam_erlang_ffi:new_selector(),
    _pipe@1 = gleam@erlang@process:selecting(
        _pipe,
        erlang:element(4, Task1),
        fun(Field@0) -> {m2_from_subject1, Field@0} end
    ),
    _pipe@2 = gleam@erlang@process:selecting(
        _pipe@1,
        erlang:element(4, Task2),
        fun(Field@0) -> {m2_from_subject2, Field@0} end
    ),
    _pipe@3 = gleam@erlang@process:selecting(
        _pipe@2,
        Timeout_subject,
        fun gleam@function:identity/1
    ),
    try_await2_loop(_pipe@3, none, none, Timer).

-file("src/gleam/otp/task.gleam", 230).
-spec try_await3_loop(
    gleam@erlang@process:selector(message3(GVA, GVB, GVC)),
    gleam@option:option({ok, GVA} | {error, await_error()}),
    gleam@option:option({ok, GVB} | {error, await_error()}),
    gleam@option:option({ok, GVC} | {error, await_error()}),
    gleam@erlang@process:timer()
) -> {{ok, GVA} | {error, await_error()},
    {ok, GVB} | {error, await_error()},
    {ok, GVC} | {error, await_error()}}.
try_await3_loop(Selector, T1, T2, T3, Timer) ->
    case {T1, T2, T3} of
        {{some, T1@1}, {some, T2@1}, {some, T3@1}} ->
            gleam@erlang@process:cancel_timer(Timer),
            {T1@1, T2@1, T3@1};

        {_, _, _} ->
            case gleam_erlang_ffi:select(Selector) of
                {m3_from_subject1, X} ->
                    T1@2 = {some, {ok, X}},
                    try_await3_loop(Selector, T1@2, T2, T3, Timer);

                {m3_from_subject2, X@1} ->
                    T2@2 = {some, {ok, X@1}},
                    try_await3_loop(Selector, T1, T2@2, T3, Timer);

                {m3_from_subject3, X@2} ->
                    T3@2 = {some, {ok, X@2}},
                    try_await3_loop(Selector, T1, T2, T3@2, Timer);

                m3_timeout ->
                    {gleam@option:unwrap(T1, {error, timeout}),
                        gleam@option:unwrap(T2, {error, timeout}),
                        gleam@option:unwrap(T3, {error, timeout})}
            end
    end.

-file("src/gleam/otp/task.gleam", 209).
?DOC(
    " Wait for the values computed by multiple tasks.\n"
    "\n"
    " For each task, if the a value is not received before the timeout has\n"
    " elapsed then an error is returned.\n"
).
-spec try_await3(task(GUO), task(GUQ), task(GUS), integer()) -> {{ok, GUO} |
        {error, await_error()},
    {ok, GUQ} | {error, await_error()},
    {ok, GUS} | {error, await_error()}}.
try_await3(Task1, Task2, Task3, Timeout) ->
    assert_owner(Task1),
    assert_owner(Task2),
    assert_owner(Task3),
    Timeout_subject = gleam@erlang@process:new_subject(),
    Timer = gleam@erlang@process:send_after(
        Timeout_subject,
        Timeout,
        m3_timeout
    ),
    _pipe = gleam_erlang_ffi:new_selector(),
    _pipe@1 = gleam@erlang@process:selecting(
        _pipe,
        erlang:element(4, Task1),
        fun(Field@0) -> {m3_from_subject1, Field@0} end
    ),
    _pipe@2 = gleam@erlang@process:selecting(
        _pipe@1,
        erlang:element(4, Task2),
        fun(Field@0) -> {m3_from_subject2, Field@0} end
    ),
    _pipe@3 = gleam@erlang@process:selecting(
        _pipe@2,
        erlang:element(4, Task3),
        fun(Field@0) -> {m3_from_subject3, Field@0} end
    ),
    _pipe@4 = gleam@erlang@process:selecting(
        _pipe@3,
        Timeout_subject,
        fun gleam@function:identity/1
    ),
    try_await3_loop(_pipe@4, none, none, none, Timer).

-file("src/gleam/otp/task.gleam", 312).
-spec try_await4_loop(
    gleam@erlang@process:selector(message4(GWM, GWN, GWO, GWP)),
    gleam@option:option({ok, GWM} | {error, await_error()}),
    gleam@option:option({ok, GWN} | {error, await_error()}),
    gleam@option:option({ok, GWO} | {error, await_error()}),
    gleam@option:option({ok, GWP} | {error, await_error()}),
    gleam@erlang@process:timer()
) -> {{ok, GWM} | {error, await_error()},
    {ok, GWN} | {error, await_error()},
    {ok, GWO} | {error, await_error()},
    {ok, GWP} | {error, await_error()}}.
try_await4_loop(Selector, T1, T2, T3, T4, Timer) ->
    case {T1, T2, T3, T4} of
        {{some, T1@1}, {some, T2@1}, {some, T3@1}, {some, T4@1}} ->
            gleam@erlang@process:cancel_timer(Timer),
            {T1@1, T2@1, T3@1, T4@1};

        {_, _, _, _} ->
            case gleam_erlang_ffi:select(Selector) of
                {m4_from_subject1, X} ->
                    T1@2 = {some, {ok, X}},
                    try_await4_loop(Selector, T1@2, T2, T3, T4, Timer);

                {m4_from_subject2, X@1} ->
                    T2@2 = {some, {ok, X@1}},
                    try_await4_loop(Selector, T1, T2@2, T3, T4, Timer);

                {m4_from_subject3, X@2} ->
                    T3@2 = {some, {ok, X@2}},
                    try_await4_loop(Selector, T1, T2, T3@2, T4, Timer);

                {m4_from_subject4, X@3} ->
                    T4@2 = {some, {ok, X@3}},
                    try_await4_loop(Selector, T1, T2, T3, T4@2, Timer);

                m4_timeout ->
                    {gleam@option:unwrap(T1, {error, timeout}),
                        gleam@option:unwrap(T2, {error, timeout}),
                        gleam@option:unwrap(T3, {error, timeout}),
                        gleam@option:unwrap(T4, {error, timeout})}
            end
    end.

-file("src/gleam/otp/task.gleam", 284).
?DOC(
    " Wait for the values computed by multiple tasks.\n"
    "\n"
    " For each task, if the a value is not received before the timeout has\n"
    " elapsed then an error is returned.\n"
).
-spec try_await4(task(GVW), task(GVY), task(GWA), task(GWC), integer()) -> {{ok,
            GVW} |
        {error, await_error()},
    {ok, GVY} | {error, await_error()},
    {ok, GWA} | {error, await_error()},
    {ok, GWC} | {error, await_error()}}.
try_await4(Task1, Task2, Task3, Task4, Timeout) ->
    assert_owner(Task1),
    assert_owner(Task2),
    assert_owner(Task3),
    Timeout_subject = gleam@erlang@process:new_subject(),
    Timer = gleam@erlang@process:send_after(
        Timeout_subject,
        Timeout,
        m4_timeout
    ),
    _pipe = gleam_erlang_ffi:new_selector(),
    _pipe@1 = gleam@erlang@process:selecting(
        _pipe,
        erlang:element(4, Task1),
        fun(Field@0) -> {m4_from_subject1, Field@0} end
    ),
    _pipe@2 = gleam@erlang@process:selecting(
        _pipe@1,
        erlang:element(4, Task2),
        fun(Field@0) -> {m4_from_subject2, Field@0} end
    ),
    _pipe@3 = gleam@erlang@process:selecting(
        _pipe@2,
        erlang:element(4, Task3),
        fun(Field@0) -> {m4_from_subject3, Field@0} end
    ),
    _pipe@4 = gleam@erlang@process:selecting(
        _pipe@3,
        erlang:element(4, Task4),
        fun(Field@0) -> {m4_from_subject4, Field@0} end
    ),
    _pipe@5 = gleam@erlang@process:selecting(
        _pipe@4,
        Timeout_subject,
        fun gleam@function:identity/1
    ),
    try_await4_loop(_pipe@5, none, none, none, none, Timer).

-file("src/gleam/otp/task.gleam", 441).
-spec dict_to_list_loop(
    gleam@dict:dict(integer(), GYJ),
    GYJ,
    integer(),
    list(GYJ)
) -> list(GYJ).
dict_to_list_loop(Dict, Default, Index, List) ->
    case Index < 0 of
        true ->
            List;

        false ->
            Value@1 = case gleam_stdlib:map_get(Dict, Index) of
                {error, _} ->
                    Default;

                {ok, Value} ->
                    Value
            end,
            dict_to_list_loop(Dict, Default, Index - 1, [Value@1 | List])
    end.

-file("src/gleam/otp/task.gleam", 437).
?DOC(
    " Given a dict returns a list with size `sized` where each item at index `i`\n"
    " is `dict.get(dict, i) |> result.unwrap(default)`.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " dict.from_list([#(1, \"a\"), #(4, \"b\")])\n"
    " |> dict_to_list(5, \" \")\n"
    " // -> [\" \", \"a\", \" \", \" \", \"b\"]\n"
    " ```\n"
).
-spec dict_to_list(gleam@dict:dict(integer(), GYF), integer(), GYF) -> list(GYF).
dict_to_list(Dict, Sized, Default) ->
    dict_to_list_loop(Dict, Default, Sized - 1, []).

-file("src/gleam/otp/task.gleam", 401).
-spec try_await_all_loop(
    gleam@dict:dict(integer(), {ok, GXV} | {error, await_error()}),
    integer(),
    gleam@erlang@process:timer(),
    gleam@erlang@process:selector(message(GXV))
) -> list({ok, GXV} | {error, await_error()}).
try_await_all_loop(Values, Tasks_count, Timer, Selector) ->
    case maps:size(Values) =:= Tasks_count of
        true ->
            gleam@erlang@process:cancel_timer(Timer),
            dict_to_list(Values, Tasks_count, {error, timeout});

        false ->
            case gleam_erlang_ffi:select(Selector) of
                message_timeout ->
                    dict_to_list(Values, Tasks_count, {error, timeout});

                {message, Index, Value} ->
                    Values@1 = gleam@dict:insert(Values, Index, {ok, Value}),
                    try_await_all_loop(Values@1, Tasks_count, Timer, Selector)
            end
    end.

-file("src/gleam/otp/task.gleam", 374).
?DOC(
    " Wait for the values computed by multiple tasks.\n"
    "\n"
    " For each task, if the a value is not received before the timeout has\n"
    " elapsed then an error is returned.\n"
).
-spec try_await_all(list(task(GXP)), integer()) -> list({ok, GXP} |
    {error, await_error()}).
try_await_all(Tasks, Timeout) ->
    {Selector@2, Tasks_count@1} = begin
        Acc = {gleam_erlang_ffi:new_selector(), 0},
        gleam@list:index_fold(
            Tasks,
            Acc,
            fun(_use0, Task, Index) ->
                {Selector, Tasks_count} = _use0,
                assert_owner(Task),
                Selector@1 = gleam@erlang@process:selecting(
                    Selector,
                    erlang:element(4, Task),
                    fun(_capture) -> {message, Index, _capture} end
                ),
                {Selector@1, Tasks_count + 1}
            end
        )
    end,
    Timeout_subject = gleam@erlang@process:new_subject(),
    Timer = gleam@erlang@process:send_after(
        Timeout_subject,
        Timeout,
        message_timeout
    ),
    Selector@3 = gleam@erlang@process:selecting(
        Selector@2,
        Timeout_subject,
        fun gleam@function:identity/1
    ),
    try_await_all_loop(maps:new(), Tasks_count@1, Timer, Selector@3).
