use crate::raft::storage::{GameEventRequest, GameRaftTypeConfig, NodeId};
use openraft::{Entry, EntryPayload, LogId, Vote};

// Re-export generated proto types
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/raft.rs"));
}

use proto::{
    AppendEntriesRequest as ProtoAppendEntriesRequest,
    AppendEntriesResponse as ProtoAppendEntriesResponse,
    InstallSnapshotRequest as ProtoInstallSnapshotRequest,
    InstallSnapshotResponse as ProtoInstallSnapshotResponse, LogEntry as ProtoLogEntry,
    VoteRequest as ProtoVoteRequest, VoteResponse as ProtoVoteResponse,
};

/// Convert OpenRaft AppendEntriesRequest to Proto
impl From<openraft::raft::AppendEntriesRequest<GameRaftTypeConfig>> for ProtoAppendEntriesRequest {
    fn from(req: openraft::raft::AppendEntriesRequest<GameRaftTypeConfig>) -> Self {
        ProtoAppendEntriesRequest {
            term: req.vote.leader_id().term,
            leader_id: req.vote.leader_id().node_id,
            prev_log_index: req.prev_log_id.as_ref().map(|id| id.index).unwrap_or(0),
            prev_log_term: req
                .prev_log_id
                .as_ref()
                .map(|id| id.leader_id.term)
                .unwrap_or(0),
            entries: req
                .entries
                .into_iter()
                .map(|e| e.into())
                .collect(),
            leader_commit: req
                .leader_commit
                .as_ref()
                .map(|id| id.index)
                .unwrap_or(0),
        }
    }
}

/// Convert Proto AppendEntriesRequest to OpenRaft
impl TryFrom<ProtoAppendEntriesRequest> for openraft::raft::AppendEntriesRequest<GameRaftTypeConfig> {
    type Error = anyhow::Error;

    fn try_from(req: ProtoAppendEntriesRequest) -> Result<Self, Self::Error> {
        let vote = Vote::new(req.term, req.leader_id);

        let prev_log_id = if req.prev_log_index > 0 {
            Some(LogId::new(
                openraft::LeaderId::new(req.prev_log_term, req.leader_id),
                req.prev_log_index,
            ))
        } else {
            None
        };

        let entries: Result<Vec<_>, _> = req
            .entries
            .into_iter()
            .map(|e| e.try_into())
            .collect();

        let leader_commit = if req.leader_commit > 0 {
            Some(LogId::new(
                openraft::LeaderId::new(req.term, req.leader_id),
                req.leader_commit,
            ))
        } else {
            None
        };

        Ok(Self {
            vote,
            prev_log_id,
            entries: entries?,
            leader_commit,
        })
    }
}

/// Convert OpenRaft AppendEntriesResponse to Proto
/// Maps: Success/PartialSuccess -> success=true, Conflict/HigherVote -> success=false
impl From<openraft::raft::AppendEntriesResponse<NodeId>> for ProtoAppendEntriesResponse {
    fn from(resp: openraft::raft::AppendEntriesResponse<NodeId>) -> Self {
        use openraft::raft::AppendEntriesResponse::*;

        match resp {
            Success | PartialSuccess(_) => ProtoAppendEntriesResponse {
                term: 0, // Success responses don't carry term info
                success: true,
            },
            Conflict => ProtoAppendEntriesResponse {
                term: 0,
                success: false,
            },
            HigherVote(vote) => ProtoAppendEntriesResponse {
                term: vote.leader_id().term,
                success: false,
            },
        }
    }
}

/// Convert Proto AppendEntriesResponse to OpenRaft
/// Simple proto response maps to Success or Conflict
impl From<ProtoAppendEntriesResponse> for openraft::raft::AppendEntriesResponse<NodeId> {
    fn from(resp: ProtoAppendEntriesResponse) -> Self {
        if resp.success {
            openraft::raft::AppendEntriesResponse::Success
        } else {
            // If term is provided, it might be a HigherVote, otherwise Conflict
            if resp.term > 0 {
                openraft::raft::AppendEntriesResponse::HigherVote(Vote::new(resp.term, 0))
            } else {
                openraft::raft::AppendEntriesResponse::Conflict
            }
        }
    }
}

/// Convert OpenRaft VoteRequest to Proto
impl From<openraft::raft::VoteRequest<NodeId>> for ProtoVoteRequest {
    fn from(req: openraft::raft::VoteRequest<NodeId>) -> Self {
        ProtoVoteRequest {
            term: req.vote.leader_id().term,
            candidate_id: req.vote.leader_id().node_id,
            last_log_index: req.last_log_id.as_ref().map(|id| id.index).unwrap_or(0),
            last_log_term: req
                .last_log_id
                .as_ref()
                .map(|id| id.leader_id.term)
                .unwrap_or(0),
        }
    }
}

/// Convert Proto VoteRequest to OpenRaft
impl From<ProtoVoteRequest> for openraft::raft::VoteRequest<NodeId> {
    fn from(req: ProtoVoteRequest) -> Self {
        let vote = Vote::new(req.term, req.candidate_id);

        let last_log_id = if req.last_log_index > 0 {
            Some(LogId::new(
                openraft::LeaderId::new(req.last_log_term, req.candidate_id),
                req.last_log_index,
            ))
        } else {
            None
        };

        Self { vote, last_log_id }
    }
}

/// Convert OpenRaft VoteResponse to Proto
impl From<openraft::raft::VoteResponse<NodeId>> for ProtoVoteResponse {
    fn from(resp: openraft::raft::VoteResponse<NodeId>) -> Self {
        ProtoVoteResponse {
            term: resp.vote.leader_id().term,
            vote_granted: resp.vote_granted,
        }
    }
}

/// Convert Proto VoteResponse to OpenRaft
impl From<ProtoVoteResponse> for openraft::raft::VoteResponse<NodeId> {
    fn from(resp: ProtoVoteResponse) -> Self {
        Self {
            vote: Vote::new(resp.term, 0), // Node ID not included in response
            vote_granted: resp.vote_granted,
            last_log_id: None,
        }
    }
}

/// Convert OpenRaft InstallSnapshotRequest to Proto
impl From<openraft::raft::InstallSnapshotRequest<GameRaftTypeConfig>>
    for ProtoInstallSnapshotRequest
{
    fn from(req: openraft::raft::InstallSnapshotRequest<GameRaftTypeConfig>) -> Self {
        ProtoInstallSnapshotRequest {
            term: req.vote.leader_id().term,
            leader_id: req.vote.leader_id().node_id,
            last_included_index: req.meta.last_log_id.as_ref().map(|id| id.index).unwrap_or(0),
            last_included_term: req
                .meta
                .last_log_id
                .as_ref()
                .map(|id| id.leader_id.term)
                .unwrap_or(0),
            data: req.data,
            done: req.done,
        }
    }
}

/// Convert Proto InstallSnapshotRequest to OpenRaft
impl TryFrom<ProtoInstallSnapshotRequest>
    for openraft::raft::InstallSnapshotRequest<GameRaftTypeConfig>
{
    type Error = anyhow::Error;

    fn try_from(req: ProtoInstallSnapshotRequest) -> Result<Self, Self::Error> {
        let vote = Vote::new(req.term, req.leader_id);

        let last_log_id = if req.last_included_index > 0 {
            Some(LogId::new(
                openraft::LeaderId::new(req.last_included_term, req.leader_id),
                req.last_included_index,
            ))
        } else {
            None
        };

        let meta = openraft::SnapshotMeta {
            last_log_id,
            last_membership: Default::default(),
            snapshot_id: format!("snapshot-{}", req.last_included_index),
        };

        Ok(Self {
            vote,
            meta,
            offset: 0,
            data: req.data,
            done: req.done,
        })
    }
}

/// Convert OpenRaft InstallSnapshotResponse to Proto
impl From<openraft::raft::InstallSnapshotResponse<NodeId>>
    for ProtoInstallSnapshotResponse
{
    fn from(resp: openraft::raft::InstallSnapshotResponse<NodeId>) -> Self {
        ProtoInstallSnapshotResponse {
            term: resp.vote.leader_id().term,
        }
    }
}

/// Convert Proto InstallSnapshotResponse to OpenRaft
impl From<ProtoInstallSnapshotResponse>
    for openraft::raft::InstallSnapshotResponse<NodeId>
{
    fn from(resp: ProtoInstallSnapshotResponse) -> Self {
        Self {
            vote: Vote::new(resp.term, 0),
        }
    }
}

/// Convert OpenRaft Entry to Proto LogEntry
impl From<Entry<GameRaftTypeConfig>> for ProtoLogEntry {
    fn from(entry: Entry<GameRaftTypeConfig>) -> Self {
        let data = match &entry.payload {
            EntryPayload::Normal(request) => {
                bincode::serialize(request).unwrap_or_default()
            }
            EntryPayload::Membership(_) => {
                // Serialize membership changes as empty for now
                vec![]
            }
            EntryPayload::Blank => vec![],
        };

        ProtoLogEntry {
            index: entry.log_id.index,
            term: entry.log_id.leader_id.term,
            data,
        }
    }
}

/// Convert Proto LogEntry to OpenRaft Entry
impl TryFrom<ProtoLogEntry> for Entry<GameRaftTypeConfig> {
    type Error = anyhow::Error;

    fn try_from(entry: ProtoLogEntry) -> Result<Self, Self::Error> {
        let payload = if entry.data.is_empty() {
            EntryPayload::Blank
        } else {
            let request: GameEventRequest = bincode::deserialize(&entry.data)
                .map_err(|e| anyhow::anyhow!("Failed to deserialize log entry: {}", e))?;
            EntryPayload::Normal(request)
        };

        Ok(Entry {
            log_id: LogId::new(
                openraft::LeaderId::new(entry.term, 0), // Node ID not stored in proto
                entry.index,
            ),
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameEvent;

    #[test]
    fn test_vote_request_roundtrip() {
        let original = openraft::raft::VoteRequest::<NodeId> {
            vote: Vote::new(5, 123),
            last_log_id: Some(LogId::new(openraft::LeaderId::new(4, 100), 42)),
        };

        let proto: ProtoVoteRequest = original.clone().into();
        let converted: openraft::raft::VoteRequest<NodeId> = proto.into();

        assert_eq!(original.vote.leader_id().term, converted.vote.leader_id().term);
        assert_eq!(original.vote.leader_id().node_id, converted.vote.leader_id().node_id);
        assert_eq!(
            original.last_log_id.as_ref().map(|id| id.index),
            converted.last_log_id.as_ref().map(|id| id.index)
        );
    }

    #[test]
    fn test_log_entry_roundtrip() {
        use crate::game::events::NodeCoord;

        let event = GameEvent::PlayerJoin {
            player_id: 12345,
            name: "Alice".to_string(),
            capital_coord: NodeCoord::new(0, 0),
            node_ip: "10.0.0.1".to_string(),
            timestamp: 1234567890,
        };

        let original = Entry::<GameRaftTypeConfig> {
            log_id: LogId::new(openraft::LeaderId::new(3, 100), 10),
            payload: EntryPayload::Normal(GameEventRequest { event: event.clone() }),
        };

        let proto: ProtoLogEntry = original.clone().into();
        let converted: Entry<GameRaftTypeConfig> = proto.try_into().unwrap();

        assert_eq!(original.log_id.index, converted.log_id.index);
        assert_eq!(original.log_id.leader_id.term, converted.log_id.leader_id.term);

        if let EntryPayload::Normal(req) = converted.payload {
            if let GameEvent::PlayerJoin { player_id, name, .. } = req.event {
                assert_eq!(player_id, 12345);
                assert_eq!(name, "Alice");
            } else {
                panic!("Wrong event type");
            }
        } else {
            panic!("Wrong payload type");
        }
    }

    #[test]
    fn test_append_entries_response_success() {
        let resp = openraft::raft::AppendEntriesResponse::<NodeId>::Success;
        let proto: ProtoAppendEntriesResponse = resp.into();
        assert_eq!(proto.success, true);
    }

    #[test]
    fn test_append_entries_response_conflict() {
        let resp = openraft::raft::AppendEntriesResponse::<NodeId>::Conflict;
        let proto: ProtoAppendEntriesResponse = resp.into();
        assert_eq!(proto.success, false);
    }
}
