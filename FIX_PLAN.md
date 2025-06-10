# Fix Plan for Blixard Issues

## Issue 1: Three-node Raft test failing
**Root cause**: The network layer isn't properly connecting Raft nodes

### What's missing:
1. Network messages aren't being delivered between nodes
2. RaftNode needs to handle incoming network messages
3. Nodes need to actually connect to each other on startup

### Fix:
1. In `RaftNode::run()`, process messages from `network.incoming_rx`
2. When sending Raft messages, use `network.send_message()` 
3. On node startup, establish connections to peers

## Issue 2: Node startup has no output
**Root cause**: The node starts but immediately blocks without logging

### Fix:
1. Add startup logging in `Node::run()`
2. Add periodic status logging
3. Ensure tracing subscriber is initialized properly

## Issue 3: Network integration incomplete

### Current state:
- `RaftNetwork` can accept connections and parse messages
- `RaftNode` generates messages but doesn't send them
- No automatic peer connection on startup

### Fix needed:
1. Connect `RaftNode` to use `RaftNetwork` for message sending
2. Add peer connection logic on startup
3. Wire up the message flow:
   - RaftNode → Network (outgoing)
   - Network → RaftNode (incoming)

## Implementation Steps:

### Step 1: Fix RaftNode message handling
```rust
// In RaftNode::run(), add:
tokio::select! {
    // Handle incoming network messages
    Some(msg) = self.network.incoming_rx.recv() => {
        self.raw_node.step(msg)?;
    }
    // ... existing code
}
```

### Step 2: Send Raft messages via network
```rust
// In RaftNode::run(), when processing ready.messages:
for msg in ready.messages.drain(..) {
    if let Some(peer_addr) = self.peers.get(&msg.to) {
        self.network.send_message(msg.to, msg).await?;
    }
}
```

### Step 3: Connect to peers on startup
```rust
// In Node::run(), before starting Raft:
for (peer_id, peer_addr) in &self.initial_peers {
    // Network should attempt connection
    // This might need a new method in RaftNetwork
}
```

### Step 4: Add logging
```rust
// In main.rs:
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env()
        .add_directive("blixard=info".parse()?)
        .add_directive("raft=info".parse()?))
    .init();
```

## Quick wins:
1. Add println! statements to see where execution stops
2. Add timeout to tests to prevent hanging
3. Use RUST_LOG=debug when running to see more details