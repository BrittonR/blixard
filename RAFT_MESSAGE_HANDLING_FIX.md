# Raft Message Handling Fix

## Issue
MsgAppendResponse messages from follower nodes were not being sent to the leader, causing cluster formation to fail.

## Symptoms
- Logs showed "Sending from 2 to 1, msg: Message { msg_type: MsgAppendResponse..." but immediately after showed "Ready state details... messages: 0"
- The message was being generated but not appearing in the ready state
- This prevented the leader from knowing that followers had processed append entries

## Root Cause
In raft-rs, not all messages trigger the `has_ready()` state. Some response messages (like MsgAppendResponse) are generated immediately when `step()` is called but don't make the node "ready". The previous implementation only checked for messages in two scenarios:
1. Inside `on_ready()` when `has_ready()` returned true
2. When `has_ready()` returned false (but this was too late)

## Solution
Check for and send messages immediately after operations that might generate them:
1. After `node.step(msg)` - handles response messages
2. After `node.tick()` - handles heartbeat messages

### Code Changes

1. **In `handle_raft_message()`**: Added message checking after `step()`
```rust
// After stepping a message, check if there are any response messages to send
let msgs = node.raft.msgs.drain(..).collect::<Vec<_>>();
if !msgs.is_empty() {
    for msg in msgs {
        self.send_raft_message(msg).await?;
    }
}
```

2. **In `tick()`**: Added message checking after `tick()`
```rust
// After tick, check if there are any messages to send (e.g., heartbeats)
let msgs = node.raft.msgs.drain(..).collect::<Vec<_>>();
if !msgs.is_empty() {
    for msg in msgs {
        self.send_raft_message(msg).await?;
    }
}
```

3. **Simplified `on_ready()`**: Removed the redundant message checking since messages are now handled immediately

## References
- raft-rs examples: https://github.com/tikv/raft-rs/blob/master/examples/five_mem_node/main.rs
- The five_mem_node example shows a pattern of checking messages after step operations

## Testing
This fix should allow:
- Followers to properly respond to append entries from the leader
- The leader to track follower progress and advance the commit index
- Proper cluster formation with 2+ nodes