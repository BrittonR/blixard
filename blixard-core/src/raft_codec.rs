use crate::common::error_context::SerializationContext;
use crate::error::{BlixardError, BlixardResult};
use protobuf::Message as ProtobufMessage;
use raft::prelude::*;

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
    if data.len() < 26 {
        // Minimum size: 8+8+1+4+4+1
        return Err::<Entry, &str>("insufficient data").deserialize_context("entry", "Entry");
    }

    let mut cursor = 0;

    // Read index
    let index = u64::from_le_bytes(
        data[cursor..cursor + 8]
            .try_into()
            .deserialize_context("entry index", "u64")?,
    );
    cursor += 8;

    // Read term
    let term = u64::from_le_bytes(
        data[cursor..cursor + 8]
            .try_into()
            .deserialize_context("entry term", "u64")?,
    );
    cursor += 8;

    // Read entry type
    let entry_type = data[cursor] as i32;
    cursor += 1;

    // Read data length and ensure we have enough bytes
    if cursor + 4 > data.len() {
        return Err::<Entry, &str>("insufficient data for data length")
            .deserialize_context("entry", "Entry");
    }
    let data_len = u32::from_le_bytes(
        data[cursor..cursor + 4]
            .try_into()
            .deserialize_context("entry data length", "u32")?,
    ) as usize;
    cursor += 4;

    // Check if we have enough bytes for data
    if cursor + data_len > data.len() {
        let msg = format!(
            "insufficient data for entry data: need {} bytes, have {}",
            cursor + data_len,
            data.len()
        );
        return Err::<Entry, String>(msg).deserialize_context("entry", "Entry");
    }
    let entry_data = data[cursor..cursor + data_len].to_vec();
    cursor += data_len;

    // Read context length and ensure we have enough bytes
    if cursor + 4 > data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize entry".to_string(),
            source: "insufficient data for context length".into(),
        });
    }
    let context_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().map_err(|_| {
        BlixardError::Serialization {
            operation: "deserialize entry".to_string(),
            source: "failed to read context length bytes".into(),
        }
    })?) as usize;
    cursor += 4;

    // Check if we have enough bytes for context
    if cursor + context_len > data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize entry".to_string(),
            source: format!(
                "insufficient data for context: need {} bytes, have {}",
                cursor + context_len,
                data.len()
            )
            .into(),
        });
    }
    let context = data[cursor..cursor + context_len].to_vec();
    cursor += context_len;

    // Check if we have the sync_log byte
    if cursor >= data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize entry".to_string(),
            source: "insufficient data for sync_log".into(),
        });
    }
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
    hs.term =
        u64::from_le_bytes(
            data[0..8]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize hard state".to_string(),
                    source: "failed to read term bytes".into(),
                })?,
        );
    hs.vote =
        u64::from_le_bytes(
            data[8..16]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize hard state".to_string(),
                    source: "failed to read vote bytes".into(),
                })?,
        );
    hs.commit =
        u64::from_le_bytes(
            data[16..24]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize hard state".to_string(),
                    source: "failed to read commit bytes".into(),
                })?,
        );
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
    if data.len() < 16 {
        // Minimum: 4 length fields
        return Err(BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "insufficient data".into(),
        });
    }

    let mut cs = ConfState::default();
    let mut cursor = 0;

    // Read voters
    if cursor + 4 > data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "insufficient data for voters length".into(),
        });
    }
    let voters_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().map_err(|_| {
        BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "failed to read voters length bytes".into(),
        }
    })?) as usize;
    cursor += 4;

    for _ in 0..voters_len {
        if cursor + 8 > data.len() {
            return Err(BlixardError::Serialization {
                operation: "deserialize conf state".to_string(),
                source: "insufficient data for voter".into(),
            });
        }
        cs.voters.push(u64::from_le_bytes(
            data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize conf state".to_string(),
                    source: "failed to read voter bytes".into(),
                })?,
        ));
        cursor += 8;
    }

    // Read learners
    if cursor + 4 > data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "insufficient data for learners length".into(),
        });
    }
    let learners_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().map_err(|_| {
        BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "failed to read learners length bytes".into(),
        }
    })?) as usize;
    cursor += 4;

    for _ in 0..learners_len {
        if cursor + 8 > data.len() {
            return Err(BlixardError::Serialization {
                operation: "deserialize conf state".to_string(),
                source: "insufficient data for learner".into(),
            });
        }
        cs.learners.push(u64::from_le_bytes(
            data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize conf state".to_string(),
                    source: "failed to read learner bytes".into(),
                })?,
        ));
        cursor += 8;
    }

    // Read voters_outgoing
    if cursor + 4 > data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "insufficient data for voters_outgoing length".into(),
        });
    }
    let voters_out_len = u32::from_le_bytes(data[cursor..cursor + 4].try_into().map_err(|_| {
        BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "failed to read voters_outgoing length bytes".into(),
        }
    })?) as usize;
    cursor += 4;

    for _ in 0..voters_out_len {
        if cursor + 8 > data.len() {
            return Err(BlixardError::Serialization {
                operation: "deserialize conf state".to_string(),
                source: "insufficient data for voter_outgoing".into(),
            });
        }
        cs.voters_outgoing.push(u64::from_le_bytes(
            data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize conf state".to_string(),
                    source: "failed to read voter_outgoing bytes".into(),
                })?,
        ));
        cursor += 8;
    }

    // Read learners_next
    if cursor + 4 > data.len() {
        return Err(BlixardError::Serialization {
            operation: "deserialize conf state".to_string(),
            source: "insufficient data for learners_next length".into(),
        });
    }
    let learners_next_len =
        u32::from_le_bytes(data[cursor..cursor + 4].try_into().map_err(|_| {
            BlixardError::Serialization {
                operation: "deserialize conf state".to_string(),
                source: "failed to read learners_next length bytes".into(),
            }
        })?) as usize;
    cursor += 4;

    for _ in 0..learners_next_len {
        if cursor + 8 > data.len() {
            return Err(BlixardError::Serialization {
                operation: "deserialize conf state".to_string(),
                source: "insufficient data for learner_next".into(),
            });
        }
        cs.learners_next.push(u64::from_le_bytes(
            data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| BlixardError::Serialization {
                    operation: "deserialize conf state".to_string(),
                    source: "failed to read learner_next bytes".into(),
                })?,
        ));
        cursor += 8;
    }

    // Read auto_leave
    if cursor < data.len() {
        cs.auto_leave = data[cursor] != 0;
    }

    Ok(cs)
}

pub fn serialize_message(msg: &Message) -> BlixardResult<Vec<u8>> {
    // Raft messages have protobuf methods from raft-proto
    let bytes = msg.write_to_bytes().serialize_context("raft message")?;

    Ok(bytes)
}

pub fn deserialize_message(data: &[u8]) -> BlixardResult<Message> {
    // Raft messages have protobuf methods from raft-proto
    let mut msg = Message::new();
    msg.merge_from_bytes(data)
        .deserialize_context("raft message", "Message")?;

    Ok(msg)
}
