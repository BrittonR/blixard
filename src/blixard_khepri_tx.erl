-module(blixard_khepri_tx).
-export([atomic_vm_scheduling/4]).

% Simple mock implementation
atomic_vm_scheduling(_Store, VmId, HostId, State) ->
    io:format("MOCK: Atomic VM scheduling:~n"),
    io:format("  - VM ID: ~p~n", [VmId]),
    io:format("  - Host ID: ~p~n", [HostId]),
    io:format("  - New State: ~p~n", [State]),
    {ok, nil}.
