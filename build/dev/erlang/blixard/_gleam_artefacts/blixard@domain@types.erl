-module(blixard@domain@types).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export_type([resource_state/0, vm_type/0, resources/0, storage_volume/0, network_interface/0, tailscale_config/0, nixos_config/0, micro_vm/0, host/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/domain/types.gleam\n").

-type resource_state() :: pending |
    scheduling |
    provisioning |
    starting |
    running |
    paused |
    stopping |
    stopped |
    failed.

-type vm_type() :: persistent | serverless.

-type resources() :: {resources, integer(), integer(), integer()}.

-type storage_volume() :: {storage_volume,
        binary(),
        binary(),
        integer(),
        binary(),
        boolean()}.

-type network_interface() :: {network_interface,
        binary(),
        binary(),
        gleam@option:option(binary()),
        gleam@option:option(binary()),
        gleam@option:option(binary())}.

-type tailscale_config() :: {tailscale_config,
        boolean(),
        gleam@option:option(binary()),
        binary(),
        list(binary()),
        boolean()}.

-type nixos_config() :: {nixos_config,
        binary(),
        gleam@dict:dict(binary(), binary()),
        gleam@option:option(binary())}.

-type micro_vm() :: {micro_vm,
        binary(),
        binary(),
        gleam@option:option(binary()),
        vm_type(),
        resources(),
        resource_state(),
        gleam@option:option(binary()),
        list(storage_volume()),
        list(network_interface()),
        tailscale_config(),
        nixos_config(),
        gleam@dict:dict(binary(), binary()),
        binary(),
        binary()}.

-type host() :: {host,
        binary(),
        binary(),
        gleam@option:option(binary()),
        binary(),
        boolean(),
        resources(),
        resources(),
        list(binary()),
        boolean(),
        list(binary()),
        gleam@dict:dict(binary(), binary()),
        binary(),
        binary()}.


