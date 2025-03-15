-module(blixard@test_khepri).
-compile([no_auto_import, nowarn_unused_vars, nowarn_unused_function, nowarn_nomatch]).

-export([main/0]).

-if(?OTP_RELEASE >= 27).
-define(MODULEDOC(Str), -moduledoc(Str)).
-define(DOC(Str), -doc(Str)).
-else.
-define(MODULEDOC(Str), -compile([])).
-define(DOC(Str), -compile([])).
-endif.

?MODULEDOC(" src/blixard/test_khepri.gleam\n").

-file("src/blixard/test_khepri.gleam", 175).
-spec create_test_vm() -> blixard@domain@types:micro_vm().
create_test_vm() ->
    {micro_vm,
        <<"test-vm-1"/utf8>>,
        <<"Test VM"/utf8>>,
        {some, <<"A test VM for Khepri store testing"/utf8>>},
        persistent,
        {resources, 2, 2048, 20},
        pending,
        none,
        [{storage_volume,
                <<"test-vol-1"/utf8>>,
                <<"Root Volume"/utf8>>,
                20,
                <<"/dev/vda"/utf8>>,
                true}],
        [{network_interface,
                <<"test-nic-1"/utf8>>,
                <<"eth0"/utf8>>,
                none,
                none,
                none}],
        {tailscale_config,
            true,
            none,
            <<"test-vm-1"/utf8>>,
            [<<"test"/utf8>>, <<"development"/utf8>>],
            true},
        {nixos_config,
            <<"/etc/nixos/vm-configs/test-vm-1.nix"/utf8>>,
            maps:new(),
            none},
        maps:from_list(
            [{<<"environment"/utf8>>, <<"testing"/utf8>>},
                {<<"project"/utf8>>, <<"blixard"/utf8>>}]
        ),
        <<"2025-03-14T12:00:00Z"/utf8>>,
        <<"2025-03-14T12:00:00Z"/utf8>>}.

-file("src/blixard/test_khepri.gleam", 224).
-spec create_test_host() -> blixard@domain@types:host().
create_test_host() ->
    {host,
        <<"test-host-1"/utf8>>,
        <<"Test Host"/utf8>>,
        {some, <<"A test host for Khepri store testing"/utf8>>},
        <<"192.168.1.100"/utf8>>,
        true,
        {resources, 8, 16384, 500},
        {resources, 8, 16384, 500},
        [],
        true,
        [<<"test"/utf8>>, <<"development"/utf8>>],
        maps:from_list(
            [{<<"datacenter"/utf8>>, <<"local"/utf8>>},
                {<<"rack"/utf8>>, <<"virtual"/utf8>>}]
        ),
        <<"2025-03-14T12:00:00Z"/utf8>>,
        <<"2025-03-14T12:00:00Z"/utf8>>}.

-file("src/blixard/test_khepri.gleam", 15).
-spec main() -> nil.
main() ->
    gleam_stdlib:println(<<"Starting Khepri Store Test"/utf8>>),
    gleam_stdlib:println(<<"=========================="/utf8>>),
    gleam_stdlib:println(<<"\n1. Initializing Khepri store..."/utf8>>),
    Store_result = blixard_khepri_store:start(
        [<<"blixard@127.0.0.1"/utf8>>],
        <<"blixard_test_cluster"/utf8>>
    ),
    case Store_result of
        {ok, Store} ->
            gleam_stdlib:println(
                <<"   ✓ Khepri store initialized successfully!"/utf8>>
            ),
            gleam_stdlib:println(<<"\n2. Creating test microVM..."/utf8>>),
            Vm = create_test_vm(),
            gleam_stdlib:println(
                <<"   ✓ Created test VM with ID: "/utf8,
                    (erlang:element(2, Vm))/binary>>
            ),
            gleam_stdlib:println(<<"\n3. Storing VM in Khepri..."/utf8>>),
            case blixard_khepri_store:put_vm(Store, Vm) of
                {ok, _} ->
                    gleam_stdlib:println(
                        <<"   ✓ VM stored successfully!"/utf8>>
                    ),
                    gleam_stdlib:println(
                        <<"\n4. Retrieving VM from Khepri..."/utf8>>
                    ),
                    case blixard_khepri_store:get_vm(
                        Store,
                        erlang:element(2, Vm)
                    ) of
                        {ok, Retrieved_vm} ->
                            gleam_stdlib:println(
                                <<"   ✓ VM retrieved successfully!"/utf8>>
                            ),
                            gleam_stdlib:println(
                                <<"   ✓ VM name: "/utf8,
                                    (erlang:element(3, Retrieved_vm))/binary>>
                            ),
                            gleam_stdlib:println(
                                <<"   ✓ VM state: "/utf8,
                                    (gleam@string:inspect(
                                        erlang:element(7, Retrieved_vm)
                                    ))/binary>>
                            ),
                            gleam_stdlib:println(
                                <<"\n5. Listing all VMs..."/utf8>>
                            ),
                            case blixard_khepri_store:list_vms(Store) of
                                {ok, Vms} ->
                                    gleam_stdlib:println(
                                        <<<<"   ✓ Found "/utf8,
                                                (gleam@string:inspect(
                                                    erlang:length(Vms)
                                                ))/binary>>/binary,
                                            " VM(s)"/utf8>>
                                    ),
                                    gleam_stdlib:println(
                                        <<"\n6. Updating VM state to Running..."/utf8>>
                                    ),
                                    case blixard_khepri_store:update_vm_state(
                                        Store,
                                        erlang:element(2, Vm),
                                        running
                                    ) of
                                        {ok, _} ->
                                            gleam_stdlib:println(
                                                <<"   ✓ VM state updated successfully!"/utf8>>
                                            ),
                                            case blixard_khepri_store:get_vm(
                                                Store,
                                                erlang:element(2, Vm)
                                            ) of
                                                {ok, Updated_vm} ->
                                                    gleam_stdlib:println(
                                                        <<"   ✓ Updated VM state: "/utf8,
                                                            (gleam@string:inspect(
                                                                erlang:element(
                                                                    7,
                                                                    Updated_vm
                                                                )
                                                            ))/binary>>
                                                    ),
                                                    gleam_stdlib:println(
                                                        <<"\n7. Creating and storing test host..."/utf8>>
                                                    ),
                                                    Host = create_test_host(),
                                                    case blixard_khepri_store:put_host(
                                                        Store,
                                                        Host
                                                    ) of
                                                        {ok, _} ->
                                                            gleam_stdlib:println(
                                                                <<"   ✓ Host stored successfully!"/utf8>>
                                                            ),
                                                            gleam_stdlib:println(
                                                                <<"\n8. Assigning VM to host..."/utf8>>
                                                            ),
                                                            case blixard_khepri_store:assign_vm_to_host(
                                                                Store,
                                                                erlang:element(
                                                                    2,
                                                                    Vm
                                                                ),
                                                                erlang:element(
                                                                    2,
                                                                    Host
                                                                )
                                                            ) of
                                                                {ok, _} ->
                                                                    gleam_stdlib:println(
                                                                        <<"   ✓ VM assigned to host successfully!"/utf8>>
                                                                    ),
                                                                    case blixard_khepri_store:get_vm(
                                                                        Store,
                                                                        erlang:element(
                                                                            2,
                                                                            Vm
                                                                        )
                                                                    ) of
                                                                        {ok,
                                                                            Assigned_vm} ->
                                                                            gleam_stdlib:println(
                                                                                <<"   ✓ VM host_id: "/utf8,
                                                                                    (gleam@string:inspect(
                                                                                        erlang:element(
                                                                                            8,
                                                                                            Assigned_vm
                                                                                        )
                                                                                    ))/binary>>
                                                                            ),
                                                                            gleam_stdlib:println(
                                                                                <<"\n9. Cleaning up..."/utf8>>
                                                                            ),
                                                                            _ = blixard_khepri_store:delete_vm(
                                                                                Store,
                                                                                erlang:element(
                                                                                    2,
                                                                                    Vm
                                                                                )
                                                                            ),
                                                                            _ = blixard_khepri_store:delete_host(
                                                                                Store,
                                                                                erlang:element(
                                                                                    2,
                                                                                    Host
                                                                                )
                                                                            ),
                                                                            _ = blixard_khepri_store:stop(
                                                                                Store
                                                                            ),
                                                                            gleam_stdlib:println(
                                                                                <<"   ✓ Cleanup completed!"/utf8>>
                                                                            ),
                                                                            gleam_stdlib:println(
                                                                                <<"\nAll tests completed successfully! ✨"/utf8>>
                                                                            );

                                                                        {error,
                                                                            Err} ->
                                                                            gleam_stdlib:println(
                                                                                <<"   ✗ Failed to get assigned VM: "/utf8,
                                                                                    (gleam@string:inspect(
                                                                                        Err
                                                                                    ))/binary>>
                                                                            )
                                                                    end;

                                                                {error, Err@1} ->
                                                                    gleam_stdlib:println(
                                                                        <<"   ✗ Failed to assign VM to host: "/utf8,
                                                                            (gleam@string:inspect(
                                                                                Err@1
                                                                            ))/binary>>
                                                                    )
                                                            end;

                                                        {error, Err@2} ->
                                                            gleam_stdlib:println(
                                                                <<"   ✗ Failed to store host: "/utf8,
                                                                    (gleam@string:inspect(
                                                                        Err@2
                                                                    ))/binary>>
                                                            )
                                                    end;

                                                {error, Err@3} ->
                                                    gleam_stdlib:println(
                                                        <<"   ✗ Failed to get updated VM: "/utf8,
                                                            (gleam@string:inspect(
                                                                Err@3
                                                            ))/binary>>
                                                    )
                                            end;

                                        {error, Err@4} ->
                                            gleam_stdlib:println(
                                                <<"   ✗ Failed to update VM state: "/utf8,
                                                    (gleam@string:inspect(Err@4))/binary>>
                                            )
                                    end;

                                {error, Err@5} ->
                                    gleam_stdlib:println(
                                        <<"   ✗ Failed to list VMs: "/utf8,
                                            (gleam@string:inspect(Err@5))/binary>>
                                    )
                            end;

                        {error, Err@6} ->
                            gleam_stdlib:println(
                                <<"   ✗ Failed to retrieve VM: "/utf8,
                                    (gleam@string:inspect(Err@6))/binary>>
                            )
                    end;

                {error, Err@7} ->
                    gleam_stdlib:println(
                        <<"   ✗ Failed to store VM: "/utf8,
                            (gleam@string:inspect(Err@7))/binary>>
                    )
            end;

        {error, Err@8} ->
            gleam_stdlib:println(
                <<"   ✗ Failed to initialize Khepri store: "/utf8,
                    (gleam@string:inspect(Err@8))/binary>>
            )
    end.
