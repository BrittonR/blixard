//! Raft message types and routing
//!
//! This module defines the message types used for Raft communication
//! and provides utilities for message routing and handling.

use crate::error::BlixardResult;
use raft::prelude::Message;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::proposals::ProposalData;

/// Main message type for Raft communication
#[derive(Debug)]
pub enum RaftMessage {
    /// Raft protocol messages (heartbeats, votes, etc.)
    Raft(Message),
    /// Application-specific proposals
    Propose(RaftProposal),
    /// Configuration changes (add/remove nodes)
    ConfChange(RaftConfChange),
}

/// Configuration change request
#[derive(Debug)]
pub struct RaftConfChange {
    pub change_type: ConfChangeType,
    pub node_id: u64,
    pub address: String,
    pub p2p_node_id: Option<String>,
    pub p2p_addresses: Vec<String>,
    pub p2p_relay_url: Option<String>,
    pub response_tx: Option<oneshot::Sender<BlixardResult<()>>>,
}

/// Type of configuration change
#[derive(Debug, Clone, Copy)]
pub enum ConfChangeType {
    AddNode,
    RemoveNode,
}

/// Configuration change context that includes P2P info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfChangeContext {
    pub address: String,
    pub p2p_node_id: Option<String>,
    pub p2p_addresses: Vec<String>,
    pub p2p_relay_url: Option<String>,
}

/// Raft proposal wrapper
#[derive(Debug)]
pub struct RaftProposal {
    /// Unique proposal ID
    pub id: Vec<u8>,
    /// Proposal data
    pub data: ProposalData,
    /// Response channel for proposal result
    pub response_tx: Option<oneshot::Sender<BlixardResult<()>>>,
}

impl RaftProposal {
    /// Create a new proposal with a generated ID
    pub fn new(data: ProposalData) -> Self {
        Self {
            id: super::utils::generate_proposal_id(),
            data,
            response_tx: None,
        }
    }

    /// Create a proposal with a response channel
    pub fn with_response(data: ProposalData) -> (Self, oneshot::Receiver<BlixardResult<()>>) {
        let (tx, rx) = oneshot::channel();
        let proposal = Self {
            id: super::utils::generate_proposal_id(),
            data,
            response_tx: Some(tx),
        };
        (proposal, rx)
    }
}

/// Message routing information
#[derive(Debug, Clone)]
pub struct MessageRoute {
    pub from: u64,
    pub to: u64,
    pub msg_type: MessageType,
}

#[derive(Debug, Clone, Copy)]
pub enum MessageType {
    Vote,
    Heartbeat,
    Append,
    Snapshot,
    VoteResponse,
    HeartbeatResponse,
    AppendResponse,
    SnapshotResponse,
}

impl From<&Message> for MessageType {
    fn from(msg: &Message) -> Self {
        use raft::prelude::MessageType as RaftMsgType;
        match msg.msg_type() {
            RaftMsgType::MsgHup => MessageType::Vote,
            RaftMsgType::MsgBeat => MessageType::Heartbeat,
            RaftMsgType::MsgPropose => MessageType::Append,
            RaftMsgType::MsgAppend => MessageType::Append,
            RaftMsgType::MsgAppendResponse => MessageType::AppendResponse,
            RaftMsgType::MsgRequestVote => MessageType::Vote,
            RaftMsgType::MsgRequestVoteResponse => MessageType::VoteResponse,
            RaftMsgType::MsgSnapshot => MessageType::Snapshot,
            RaftMsgType::MsgHeartbeat => MessageType::Heartbeat,
            RaftMsgType::MsgHeartbeatResponse => MessageType::HeartbeatResponse,
            _ => MessageType::Heartbeat, // Default for other types
        }
    }
}

impl MessageRoute {
    pub fn from_message(msg: &Message) -> Self {
        Self {
            from: msg.from,
            to: msg.to,
            msg_type: MessageType::from(msg),
        }
    }
}
