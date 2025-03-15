-module(gleam@json).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([decode_bits/2, decode/2, parse_bits/2, parse/2, to_string/1, to_string_tree/1, to_string_builder/1, string/1, bool/1, int/1, float/1, null/0, nullable/2, object/1, preprocessed_array/1, array/2, dict/3]).
-export_type([json/0, decode_error/0]).

-type json() :: any().

-type decode_error() :: unexpected_end_of_input |
    {unexpected_byte, binary()} |
    {unexpected_sequence, binary()} |
    {unexpected_format, list(gleam@dynamic:decode_error())} |
    {unable_to_decode, list(gleam@dynamic@decode:decode_error())}.

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 99).
-spec decode_bits(
    bitstring(),
    fun((gleam@dynamic:dynamic_()) -> {ok, FDO} |
        {error, list(gleam@dynamic:decode_error())})
) -> {ok, FDO} | {error, decode_error()}.
decode_bits(Json, Decoder) ->
    gleam@result:then(
        gleam_json_ffi:decode(Json),
        fun(Dynamic_value) -> _pipe = Decoder(Dynamic_value),
            gleam@result:map_error(
                _pipe,
                fun(Field@0) -> {unexpected_format, Field@0} end
            ) end
    ).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 76).
-spec do_decode(
    binary(),
    fun((gleam@dynamic:dynamic_()) -> {ok, FDI} |
        {error, list(gleam@dynamic:decode_error())})
) -> {ok, FDI} | {error, decode_error()}.
do_decode(Json, Decoder) ->
    Bits = gleam_stdlib:identity(Json),
    decode_bits(Bits, Decoder).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 22).
-spec decode(
    binary(),
    fun((gleam@dynamic:dynamic_()) -> {ok, FCW} |
        {error, list(gleam@dynamic:decode_error())})
) -> {ok, FCW} | {error, decode_error()}.
decode(Json, Decoder) ->
    do_decode(Json, Decoder).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 128).
-spec parse_bits(bitstring(), gleam@dynamic@decode:decoder(FDS)) -> {ok, FDS} |
    {error, decode_error()}.
parse_bits(Json, Decoder) ->
    gleam@result:then(
        gleam_json_ffi:decode(Json),
        fun(Dynamic_value) ->
            _pipe = gleam@dynamic@decode:run(Dynamic_value, Decoder),
            gleam@result:map_error(
                _pipe,
                fun(Field@0) -> {unable_to_decode, Field@0} end
            )
        end
    ).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 57).
-spec do_parse(binary(), gleam@dynamic@decode:decoder(FDE)) -> {ok, FDE} |
    {error, decode_error()}.
do_parse(Json, Decoder) ->
    Bits = gleam_stdlib:identity(Json),
    parse_bits(Bits, Decoder).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 49).
-spec parse(binary(), gleam@dynamic@decode:decoder(FDA)) -> {ok, FDA} |
    {error, decode_error()}.
parse(Json, Decoder) ->
    do_parse(Json, Decoder).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 157).
-spec to_string(json()) -> binary().
to_string(Json) ->
    gleam_json_ffi:json_to_string(Json).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 198).
-spec to_string_tree(json()) -> gleam@string_tree:string_tree().
to_string_tree(Json) ->
    gleam_json_ffi:json_to_iodata(Json).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 179).
-spec to_string_builder(json()) -> gleam@string_tree:string_tree().
to_string_builder(Json) ->
    gleam_json_ffi:json_to_iodata(Json).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 209).
-spec string(binary()) -> json().
string(Input) ->
    gleam_json_ffi:string(Input).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 226).
-spec bool(boolean()) -> json().
bool(Input) ->
    gleam_json_ffi:bool(Input).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 243).
-spec int(integer()) -> json().
int(Input) ->
    gleam_json_ffi:int(Input).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 260).
-spec float(float()) -> json().
float(Input) ->
    gleam_json_ffi:float(Input).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 277).
-spec null() -> json().
null() ->
    gleam_json_ffi:null().

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 299).
-spec nullable(gleam@option:option(FDY), fun((FDY) -> json())) -> json().
nullable(Input, Inner_type) ->
    case Input of
        {some, Value} ->
            Inner_type(Value);

        none ->
            null()
    end.

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 318).
-spec object(list({binary(), json()})) -> json().
object(Entries) ->
    gleam_json_ffi:object(Entries).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 350).
-spec preprocessed_array(list(json())) -> json().
preprocessed_array(From) ->
    gleam_json_ffi:array(From).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 335).
-spec array(list(FEC), fun((FEC) -> json())) -> json().
array(Entries, Inner_type) ->
    _pipe = Entries,
    _pipe@1 = gleam@list:map(_pipe, Inner_type),
    preprocessed_array(_pipe@1).

-file("/Users/louis/src/gleam/json/src/gleam/json.gleam", 368).
-spec dict(
    gleam@dict:dict(FEG, FEH),
    fun((FEG) -> binary()),
    fun((FEH) -> json())
) -> json().
dict(Dict, Keys, Values) ->
    object(
        gleam@dict:fold(
            Dict,
            [],
            fun(Acc, K, V) -> [{Keys(K), Values(V)} | Acc] end
        )
    ).
