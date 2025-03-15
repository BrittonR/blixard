-module(gleam@json).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([decode_bits/2, decode/2, parse_bits/2, parse/2, to_string/1, to_string_tree/1, to_string_builder/1, string/1, bool/1, int/1, float/1, null/0, nullable/2, object/1, preprocessed_array/1, array/2, dict/3]).
-export_type([json/0, decode_error/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

-type json() :: any().

-type decode_error() :: unexpected_end_of_input |
    {unexpected_byte, binary()} |
    {unexpected_sequence, binary()} |
    {unexpected_format, list(gleam@dynamic:decode_error())} |
    {unable_to_decode, list(gleam@dynamic@decode:decode_error())}.

-file("src/gleam/json.gleam", 99).
?DOC(" The same as `parse_bits`, but using the old `gleam/dynamic` decoder API.\n").
-spec decode_bits(
    bitstring(),
    fun((gleam@dynamic:dynamic_()) -> {ok, FLR} |
        {error, list(gleam@dynamic:decode_error())})
) -> {ok, FLR} | {error, decode_error()}.
decode_bits(Json, Decoder) ->
    gleam@result:then(
        gleam_json_ffi:decode(Json),
        fun(Dynamic_value) -> _pipe = Decoder(Dynamic_value),
            gleam@result:map_error(
                _pipe,
                fun(Field@0) -> {unexpected_format, Field@0} end
            ) end
    ).

-file("src/gleam/json.gleam", 76).
-spec do_decode(
    binary(),
    fun((gleam@dynamic:dynamic_()) -> {ok, FLL} |
        {error, list(gleam@dynamic:decode_error())})
) -> {ok, FLL} | {error, decode_error()}.
do_decode(Json, Decoder) ->
    Bits = gleam_stdlib:identity(Json),
    decode_bits(Bits, Decoder).

-file("src/gleam/json.gleam", 22).
?DOC(" The same as `parse`, but using the old `gleam/dynamic` decoder API.\n").
-spec decode(
    binary(),
    fun((gleam@dynamic:dynamic_()) -> {ok, FKZ} |
        {error, list(gleam@dynamic:decode_error())})
) -> {ok, FKZ} | {error, decode_error()}.
decode(Json, Decoder) ->
    do_decode(Json, Decoder).

-file("src/gleam/json.gleam", 128).
?DOC(
    " Decode a JSON bit string into dynamically typed data which can be decoded\n"
    " into typed data with the `gleam/dynamic` module.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > decode_bits(<<\"[1,2,3]\">>, decode.list(of: decode.int))\n"
    " Ok([1, 2, 3])\n"
    " ```\n"
    "\n"
    " ```gleam\n"
    " > decode_bits(<<\"[\">>, decode.list(of: decode.int))\n"
    " Error(UnexpectedEndOfInput)\n"
    " ```\n"
    "\n"
    " ```gleam\n"
    " > decode_bits(\"<<1\">>, decode.string)\n"
    " Error(UnexpectedFormat([decode.DecodeError(\"String\", \"Int\", [])]))\n"
    " ```\n"
).
-spec parse_bits(bitstring(), gleam@dynamic@decode:decoder(FLV)) -> {ok, FLV} |
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

-file("src/gleam/json.gleam", 57).
-spec do_parse(binary(), gleam@dynamic@decode:decoder(FLH)) -> {ok, FLH} |
    {error, decode_error()}.
do_parse(Json, Decoder) ->
    Bits = gleam_stdlib:identity(Json),
    parse_bits(Bits, Decoder).

-file("src/gleam/json.gleam", 49).
?DOC(
    " Decode a JSON string into dynamically typed data which can be decoded into\n"
    " typed data with the `gleam/dynamic` module.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > decode(\"[1,2,3]\", decode.list(of: decode.int))\n"
    " Ok([1, 2, 3])\n"
    " ```\n"
    "\n"
    " ```gleam\n"
    " > decode(\"[\", decode.list(of: decode.int))\n"
    " Error(UnexpectedEndOfInput)\n"
    " ```\n"
    "\n"
    " ```gleam\n"
    " > decode(\"1\", decode.string)\n"
    " Error(UnableToDecode([decode.DecodeError(\"String\", \"Int\", [])]))\n"
    " ```\n"
).
-spec parse(binary(), gleam@dynamic@decode:decoder(FLD)) -> {ok, FLD} |
    {error, decode_error()}.
parse(Json, Decoder) ->
    do_parse(Json, Decoder).

-file("src/gleam/json.gleam", 157).
?DOC(
    " Convert a JSON value into a string.\n"
    "\n"
    " Where possible prefer the `to_string_tree` function as it is faster than\n"
    " this function, and BEAM VM IO is optimised for sending `StringTree` data.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(array([1, 2, 3], of: int))\n"
    " \"[1,2,3]\"\n"
    " ```\n"
).
-spec to_string(json()) -> binary().
to_string(Json) ->
    gleam_json_ffi:json_to_string(Json).

-file("src/gleam/json.gleam", 198).
?DOC(
    " Convert a JSON value into a string tree.\n"
    "\n"
    " Where possible prefer this function to the `to_string` function as it is\n"
    " slower than this function, and BEAM VM IO is optimised for sending\n"
    " `StringTree` data.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string_tree(array([1, 2, 3], of: int))\n"
    " string_tree.from_string(\"[1,2,3]\")\n"
    " ```\n"
).
-spec to_string_tree(json()) -> gleam@string_tree:string_tree().
to_string_tree(Json) ->
    gleam_json_ffi:json_to_iodata(Json).

-file("src/gleam/json.gleam", 179).
?DOC(
    " Convert a JSON value into a string builder.\n"
    "\n"
    " Where possible prefer this function to the `to_string` function as it is\n"
    " slower than this function, and BEAM VM IO is optimised for sending\n"
    " `StringTree` data.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string_builder(array([1, 2, 3], of: int))\n"
    " string_builder.from_string(\"[1,2,3]\")\n"
    " ```\n"
).
-spec to_string_builder(json()) -> gleam@string_tree:string_tree().
to_string_builder(Json) ->
    gleam_json_ffi:json_to_iodata(Json).

-file("src/gleam/json.gleam", 209).
?DOC(
    " Encode a string into JSON, using normal JSON escaping.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(string(\"Hello!\"))\n"
    " \"\\\"Hello!\\\"\"\n"
    " ```\n"
).
-spec string(binary()) -> json().
string(Input) ->
    gleam_json_ffi:string(Input).

-file("src/gleam/json.gleam", 226).
?DOC(
    " Encode a bool into JSON.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(bool(False))\n"
    " \"false\"\n"
    " ```\n"
).
-spec bool(boolean()) -> json().
bool(Input) ->
    gleam_json_ffi:bool(Input).

-file("src/gleam/json.gleam", 243).
?DOC(
    " Encode an int into JSON.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(int(50))\n"
    " \"50\"\n"
    " ```\n"
).
-spec int(integer()) -> json().
int(Input) ->
    gleam_json_ffi:int(Input).

-file("src/gleam/json.gleam", 260).
?DOC(
    " Encode a float into JSON.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(float(4.7))\n"
    " \"4.7\"\n"
    " ```\n"
).
-spec float(float()) -> json().
float(Input) ->
    gleam_json_ffi:float(Input).

-file("src/gleam/json.gleam", 277).
?DOC(
    " The JSON value null.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(null())\n"
    " \"null\"\n"
    " ```\n"
).
-spec null() -> json().
null() ->
    gleam_json_ffi:null().

-file("src/gleam/json.gleam", 299).
?DOC(
    " Encode an optional value into JSON, using null if it is the `None` variant.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(nullable(Some(50), of: int))\n"
    " \"50\"\n"
    " ```\n"
    "\n"
    " ```gleam\n"
    " > to_string(nullable(None, of: int))\n"
    " \"null\"\n"
    " ```\n"
).
-spec nullable(gleam@option:option(FMB), fun((FMB) -> json())) -> json().
nullable(Input, Inner_type) ->
    case Input of
        {some, Value} ->
            Inner_type(Value);

        none ->
            null()
    end.

-file("src/gleam/json.gleam", 318).
?DOC(
    " Encode a list of key-value pairs into a JSON object.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(object([\n"
    "   #(\"game\", string(\"Pac-Man\")),\n"
    "   #(\"score\", int(3333360)),\n"
    " ]))\n"
    " \"{\\\"game\\\":\\\"Pac-Mac\\\",\\\"score\\\":3333360}\"\n"
    " ```\n"
).
-spec object(list({binary(), json()})) -> json().
object(Entries) ->
    gleam_json_ffi:object(Entries).

-file("src/gleam/json.gleam", 350).
?DOC(
    " Encode a list of JSON values into a JSON array.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(preprocessed_array([int(1), float(2.0), string(\"3\")]))\n"
    " \"[1, 2.0, \\\"3\\\"]\"\n"
    " ```\n"
).
-spec preprocessed_array(list(json())) -> json().
preprocessed_array(From) ->
    gleam_json_ffi:array(From).

-file("src/gleam/json.gleam", 335).
?DOC(
    " Encode a list into a JSON array.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(array([1, 2, 3], of: int))\n"
    " \"[1, 2, 3]\"\n"
    " ```\n"
).
-spec array(list(FMF), fun((FMF) -> json())) -> json().
array(Entries, Inner_type) ->
    _pipe = Entries,
    _pipe@1 = gleam@list:map(_pipe, Inner_type),
    preprocessed_array(_pipe@1).

-file("src/gleam/json.gleam", 368).
?DOC(
    " Encode a Dict into a JSON object using the supplied functions to encode\n"
    " the keys and the values respectively.\n"
    "\n"
    " ## Examples\n"
    "\n"
    " ```gleam\n"
    " > to_string(dict(dict.from_list([ #(3, 3.0), #(4, 4.0)]), int.to_string, float)\n"
    " \"{\\\"3\\\": 3.0, \\\"4\\\": 4.0}\"\n"
    " ```\n"
).
-spec dict(
    gleam@dict:dict(FMJ, FMK),
    fun((FMJ) -> binary()),
    fun((FMK) -> json())
) -> json().
dict(Dict, Keys, Values) ->
    object(
        gleam@dict:fold(
            Dict,
            [],
            fun(Acc, K, V) -> [{Keys(K), Values(V)} | Acc] end
        )
    ).
