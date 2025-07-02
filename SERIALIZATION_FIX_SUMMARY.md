# VM Service Serialization Error Fix

## Issue
The persistent serialization error was occurring when handling `CreateWithScheduling` and `SchedulePlacement` VM operation requests in the `IrohVmService`.

## Root Cause
In `IrohVmService::handle_vm_operation()`, the `CreateWithScheduling` and `SchedulePlacement` variants were returning incorrect response types:
- `CreateWithScheduling` was returning `VmOperationResponse::Create` instead of `VmOperationResponse::CreateWithScheduling`
- `SchedulePlacement` was returning `VmOperationResponse::Create` instead of `VmOperationResponse::SchedulePlacement`

This mismatch caused deserialization failures when clients expected the correct response variant.

## Fix Applied
Updated the `handle_vm_operation` method in `/home/brittonr/git/blixard/blixard-core/src/transport/iroh_vm_service.rs` to:

1. Call the proper service methods (`create_vm_with_scheduling` and `schedule_vm_placement`)
2. Return the correct response variants with all required fields
3. Handle error cases appropriately

## Changes Made
- Fixed `CreateWithScheduling` to return `VmOperationResponse::CreateWithScheduling` with node ID and placement decision
- Fixed `SchedulePlacement` to return `VmOperationResponse::SchedulePlacement` with score, reason, and alternative nodes
- Cleaned up unused imports in affected files

## Verification
The fix ensures that:
- Request/response types match between client and server
- All enum variants are properly handled
- Serialization/deserialization works correctly with bincode

The serialization is handled by bincode (not postcard as mentioned in the error description), and the fix ensures proper type matching across the RPC boundary.