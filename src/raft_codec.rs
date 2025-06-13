use raft::prelude::*;
use crate::error::{BlixardError, BlixardResult};

// Wrapper functions for Raft type serialization/deserialization
// Since raft types don't expose protobuf methods directly, we need to work around this

pub fn serialize_entry(entry: &Entry) -> BlixardResult<Vec<u8>> {
    // For now, we'll use a simple encoding scheme
    // Format: [index:8][term:8][entry_type:1][data_len:4][data][context_len:4][context]
    let mut buf = Vec::new();
    
    // Write index
    buf.extend_from_slice(&entry.index.to_le_bytes());
    
    // Write term
    buf.extend_from_slice(&entry.term.to_le_bytes());
    
    // Write entry type
    buf.push(entry.entry_type as u8);
    
    // Write data
    buf.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&entry.data);
    
    // Write context
    buf.extend_from_slice(&(entry.context.len() as u32).to_le_bytes());
    buf.extend_from_slice(&entry.context);
    
    // Write sync_log flag
    buf.push(if entry.sync_log { 1 } else { 0 });
    
    Ok(buf)
}

pub fn deserialize_entry(data: &[u8]) -> BlixardResult<Entry> {
    if data.len() < 21 { // Minimum size
        return Err(BlixardError::Serialization {
            operation: "deserialize entry".to_string(),
            source: "insufficient data".into(),
        });
    }
    
    let mut cursor = 0;
    
    // Read index
    let index = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    
    // Read term
    let term = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    
    // Read entry type
    let entry_type = data[cursor] as i32;
    cursor += 1;
    
    // Read data
    let data_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    let entry_data = data[cursor..cursor+data_len].to_vec();
    cursor += data_len;
    
    // Read context
    let context_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    let context = data[cursor..cursor+context_len].to_vec();
    cursor += context_len;
    
    // Read sync_log
    let sync_log = data[cursor] != 0;
    
    Ok(Entry {
        entry_type,
        term,
        index,
        data: entry_data,
        context,
        sync_log,
    })
}

pub fn serialize_hard_state(hs: &HardState) -> BlixardResult<Vec<u8>> {
    // Format: [term:8][vote:8][commit:8]
    let mut buf = Vec::with_capacity(24);
    buf.extend_from_slice(&hs.term.to_le_bytes());
    buf.extend_from_slice(&hs.vote.to_le_bytes());
    buf.extend_from_slice(&hs.commit.to_le_bytes());
    Ok(buf)
}

pub fn deserialize_hard_state(data: &[u8]) -> BlixardResult<HardState> {
    if data.len() != 24 {
        return Err(BlixardError::Serialization {
            operation: "deserialize hard state".to_string(),
            source: "invalid data length".into(),
        });
    }
    
    let mut hs = HardState::default();
    hs.term = u64::from_le_bytes(data[0..8].try_into().unwrap());
    hs.vote = u64::from_le_bytes(data[8..16].try_into().unwrap());
    hs.commit = u64::from_le_bytes(data[16..24].try_into().unwrap());
    Ok(hs)
}

pub fn serialize_conf_state(cs: &ConfState) -> BlixardResult<Vec<u8>> {
    // ConfState has: voters, learners, voters_outgoing, learners_next, auto_leave
    let mut buf = Vec::new();
    
    // Write voters
    buf.extend_from_slice(&(cs.voters.len() as u32).to_le_bytes());
    for &voter in &cs.voters {
        buf.extend_from_slice(&voter.to_le_bytes());
    }
    
    // Write learners
    buf.extend_from_slice(&(cs.learners.len() as u32).to_le_bytes());
    for &learner in &cs.learners {
        buf.extend_from_slice(&learner.to_le_bytes());
    }
    
    // Write voters_outgoing
    buf.extend_from_slice(&(cs.voters_outgoing.len() as u32).to_le_bytes());
    for &voter in &cs.voters_outgoing {
        buf.extend_from_slice(&voter.to_le_bytes());
    }
    
    // Write learners_next
    buf.extend_from_slice(&(cs.learners_next.len() as u32).to_le_bytes());
    for &learner in &cs.learners_next {
        buf.extend_from_slice(&learner.to_le_bytes());
    }
    
    // Write auto_leave
    buf.push(if cs.auto_leave { 1 } else { 0 });
    
    Ok(buf)
}

pub fn deserialize_conf_state(data: &[u8]) -> BlixardResult<ConfState> {
    let mut cs = ConfState::default();
    let mut cursor = 0;
    
    // Read voters
    let voters_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    for _ in 0..voters_len {
        cs.voters.push(u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap()));
        cursor += 8;
    }
    
    // Read learners
    let learners_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    for _ in 0..learners_len {
        cs.learners.push(u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap()));
        cursor += 8;
    }
    
    // Read voters_outgoing
    let voters_out_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    for _ in 0..voters_out_len {
        cs.voters_outgoing.push(u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap()));
        cursor += 8;
    }
    
    // Read learners_next
    let learners_next_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    for _ in 0..learners_next_len {
        cs.learners_next.push(u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap()));
        cursor += 8;
    }
    
    // Read auto_leave
    if cursor < data.len() {
        cs.auto_leave = data[cursor] != 0;
    }
    
    Ok(cs)
}

pub fn serialize_message(msg: &Message) -> BlixardResult<Vec<u8>> {
    // For Raft messages, we need to handle protobuf encoding
    // This is a simplified version - in production you'd use proper protobuf
    let mut buf = Vec::new();
    
    // Basic fields
    buf.extend_from_slice(&msg.msg_type.to_le_bytes());
    buf.extend_from_slice(&msg.to.to_le_bytes());
    buf.extend_from_slice(&msg.from.to_le_bytes());
    buf.extend_from_slice(&msg.term.to_le_bytes());
    buf.extend_from_slice(&msg.log_term.to_le_bytes());
    buf.extend_from_slice(&msg.index.to_le_bytes());
    buf.extend_from_slice(&msg.commit.to_le_bytes());
    buf.push(if msg.reject { 1 } else { 0 });
    buf.extend_from_slice(&msg.reject_hint.to_le_bytes());
    
    // Context
    buf.extend_from_slice(&(msg.context.len() as u32).to_le_bytes());
    buf.extend_from_slice(&msg.context);
    
    // For entries, we'll skip them for now as they're complex
    buf.extend_from_slice(&0u32.to_le_bytes()); // entries count
    
    // Snapshot - simplified
    buf.push(0); // no snapshot
    
    Ok(buf)
}

pub fn deserialize_message(data: &[u8]) -> BlixardResult<Message> {
    if data.len() < 73 { // Minimum size
        return Err(BlixardError::Serialization {
            operation: "deserialize message".to_string(),
            source: "insufficient data".into(),
        });
    }
    
    let mut msg = Message::default();
    let mut cursor = 0;
    
    // Basic fields
    msg.msg_type = i32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap());
    cursor += 4;
    msg.to = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    msg.from = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    msg.term = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    msg.log_term = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    msg.index = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    msg.commit = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    msg.reject = data[cursor] != 0;
    cursor += 1;
    msg.reject_hint = u64::from_le_bytes(data[cursor..cursor+8].try_into().unwrap());
    cursor += 8;
    
    // Context
    let context_len = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    if cursor + context_len <= data.len() {
        msg.context = data[cursor..cursor+context_len].to_vec();
        cursor += context_len;
    }
    
    // Skip entries for now
    cursor += 4; // entries count
    
    // Skip snapshot
    let _ = cursor + 1;
    
    Ok(msg)
}