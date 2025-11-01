use crate::game::{GameEvent, GameState};
use openraft::storage::{LogState, Snapshot};
use openraft::{
    Entry, EntryPayload, ErrorSubject, ErrorVerb, LogId, RaftLogReader, RaftSnapshotBuilder,
    RaftStorage, SnapshotMeta, StorageError, StoredMembership, Vote,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Node ID type
pub type NodeId = u64;

/// Application data type - game events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEventRequest {
    pub event: GameEvent,
}

/// Application response type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEventResponse {
    pub success: bool,
}

/// Snapshot data type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateSnapshot {
    pub events: Vec<GameEvent>,  // For replay/audit
    pub last_applied_log_index: u64,
}

/// Type config for OpenRaft
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GameRaftTypeConfig;

impl std::fmt::Display for GameRaftTypeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GameRaftTypeConfig")
    }
}

impl openraft::RaftTypeConfig for GameRaftTypeConfig {
    type D = GameEventRequest;
    type R = GameEventResponse;
    type NodeId = NodeId;
    type Node = ();
    type Entry = Entry<Self>;
    type SnapshotData = Cursor<Vec<u8>>;
    type AsyncRuntime = openraft::TokioRuntime;
    type Responder = openraft::impls::OneshotResponder<Self>;
}

/// In-memory storage for Raft
pub struct MemStorage {
    /// Current term and vote
    vote: Arc<RwLock<Option<Vote<NodeId>>>>,

    /// Log entries (index -> entry)
    log: Arc<RwLock<BTreeMap<u64, Entry<GameRaftTypeConfig>>>>,

    /// State machine - all committed game events
    state_machine: Arc<RwLock<GameStateMachine>>,

    /// Last snapshot
    snapshot: Arc<RwLock<Option<GameStateSnapshot>>>,

    /// Snapshot metadata
    snapshot_meta: Arc<RwLock<Option<SnapshotMeta<NodeId, ()>>>>,

    /// Committed membership
    committed: Arc<RwLock<Option<StoredMembership<NodeId, ()>>>>,
}

/// Game state machine - derived state + event history
pub struct GameStateMachine {
    /// Derived game state (players, nodes, attacks, etc.)
    pub game_state: GameState,

    /// All game events in order (for replay/audit)
    pub events: Vec<GameEvent>,

    /// Last applied log index
    pub last_applied_log_index: u64,
}

impl MemStorage {
    pub fn new() -> Self {
        Self {
            vote: Arc::new(RwLock::new(None)),
            log: Arc::new(RwLock::new(BTreeMap::new())),
            state_machine: Arc::new(RwLock::new(GameStateMachine {
                game_state: GameState::new(),
                events: Vec::new(),
                last_applied_log_index: 0,
            })),
            snapshot: Arc::new(RwLock::new(None)),
            snapshot_meta: Arc::new(RwLock::new(None)),
            committed: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the state machine for reading game events
    pub fn state_machine(&self) -> Arc<RwLock<GameStateMachine>> {
        self.state_machine.clone()
    }

    pub fn clone_storage(&self) -> Self {
        Self {
            vote: self.vote.clone(),
            log: self.log.clone(),
            state_machine: self.state_machine.clone(),
            snapshot: self.snapshot.clone(),
            snapshot_meta: self.snapshot_meta.clone(),
            committed: self.committed.clone(),
        }
    }
}

impl RaftLogReader<GameRaftTypeConfig> for MemStorage {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + Send>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<GameRaftTypeConfig>>, StorageError<NodeId>> {
        let log = self.log.read().await;
        let entries = log
            .range(range)
            .map(|(_, entry)| entry.clone())
            .collect();
        Ok(entries)
    }
}

impl RaftSnapshotBuilder<GameRaftTypeConfig> for MemStorage {
    async fn build_snapshot(&mut self) -> Result<Snapshot<GameRaftTypeConfig>, StorageError<NodeId>> {
        let sm = self.state_machine.read().await;
        let snapshot_data = GameStateSnapshot {
            events: sm.events.clone(),
            last_applied_log_index: sm.last_applied_log_index,
        };

        // Serialize snapshot to bytes
        let bytes = bincode::serialize(&snapshot_data)
            .map_err(|e| {
                StorageError::from_io_error(
                    ErrorSubject::Snapshot(None),
                    ErrorVerb::Read,
                    std::io::Error::new(std::io::ErrorKind::Other, e)
                )
            })?;

        let meta = SnapshotMeta {
            last_log_id: if sm.last_applied_log_index > 0 {
                Some(LogId::new(
                    openraft::LeaderId::new(0, 0),
                    sm.last_applied_log_index
                ))
            } else {
                None
            },
            last_membership: self.committed.read().await.clone().unwrap_or_default(),
            snapshot_id: format!("snapshot-{}", sm.last_applied_log_index),
        };

        *self.snapshot.write().await = Some(snapshot_data);
        *self.snapshot_meta.write().await = Some(meta.clone());

        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(bytes)),
        })
    }
}

impl RaftStorage<GameRaftTypeConfig> for MemStorage {
    type LogReader = Self;
    type SnapshotBuilder = Self;

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        *self.vote.write().await = Some(vote.clone());
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        Ok(self.vote.read().await.clone())
    }

    async fn get_log_state(&mut self) -> Result<LogState<GameRaftTypeConfig>, StorageError<NodeId>> {
        let log = self.log.read().await;
        let last_log_id = log
            .iter()
            .last()
            .map(|(_, entry)| entry.log_id);

        let last_purged_log_id = None;

        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone_storage()
    }

    async fn append_to_log<I>(&mut self, entries: I) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<GameRaftTypeConfig>> + Send,
    {
        let mut log = self.log.write().await;
        for entry in entries {
            log.insert(entry.log_id.index, entry);
        }
        Ok(())
    }

    async fn delete_conflict_logs_since(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut log = self.log.write().await;
        log.retain(|&index, entry| {
            index < log_id.index || entry.log_id.leader_id == log_id.leader_id
        });
        Ok(())
    }

    async fn purge_logs_upto(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut log = self.log.write().await;
        log.retain(|&index, _| index > log_id.index);
        Ok(())
    }

    async fn last_applied_state(
        &mut self,
    ) -> Result<(Option<LogId<NodeId>>, StoredMembership<NodeId, ()>), StorageError<NodeId>> {
        let sm = self.state_machine.read().await;
        let last_log_id = if sm.last_applied_log_index > 0 {
            Some(LogId::new(
                openraft::LeaderId::new(0, 0),
                sm.last_applied_log_index
            ))
        } else {
            None
        };

        let committed = self.committed.read().await.clone().unwrap_or_default();

        Ok((last_log_id, committed))
    }

    async fn apply_to_state_machine(
        &mut self,
        entries: &[Entry<GameRaftTypeConfig>],
    ) -> Result<Vec<GameEventResponse>, StorageError<NodeId>> {
        let mut sm = self.state_machine.write().await;
        let mut responses = Vec::new();

        for entry in entries {
            if let EntryPayload::Normal(request) = &entry.payload {
                // Store event for replay/audit
                sm.events.push(request.event.clone());

                // Process event into derived game state
                sm.game_state.process_event(request.event.clone(), entry.log_id.index);

                sm.last_applied_log_index = entry.log_id.index;
                responses.push(GameEventResponse { success: true });
            } else {
                responses.push(GameEventResponse { success: false });
            }
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone_storage()
    }

    async fn begin_receiving_snapshot(&mut self) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, ()>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<NodeId>> {
        // Deserialize snapshot from bytes
        let bytes = snapshot.get_ref();
        let snapshot_data: GameStateSnapshot = bincode::deserialize(bytes)
            .map_err(|e| {
                StorageError::from_io_error(
                    ErrorSubject::Snapshot(Some(meta.signature())),
                    ErrorVerb::Read,
                    std::io::Error::new(std::io::ErrorKind::Other, e)
                )
            })?;

        let mut sm = self.state_machine.write().await;
        sm.events = snapshot_data.events.clone();
        sm.last_applied_log_index = snapshot_data.last_applied_log_index;

        // Rebuild game state from events
        sm.game_state = GameState::new();
        for (idx, event) in snapshot_data.events.iter().enumerate() {
            sm.game_state.process_event(event.clone(), idx as u64 + 1);
        }

        *self.snapshot.write().await = Some(snapshot_data);
        *self.snapshot_meta.write().await = Some(meta.clone());
        *self.committed.write().await = Some(meta.last_membership.clone());

        Ok(())
    }

    async fn get_current_snapshot(&mut self) -> Result<Option<Snapshot<GameRaftTypeConfig>>, StorageError<NodeId>> {
        let snapshot = self.snapshot.read().await.clone();
        let meta = self.snapshot_meta.read().await.clone();

        match (snapshot, meta) {
            (Some(data), Some(meta)) => {
                // Serialize snapshot to bytes
                let bytes = bincode::serialize(&data)
                    .map_err(|e| {
                        StorageError::from_io_error(
                            ErrorSubject::Snapshot(Some(meta.signature())),
                            ErrorVerb::Read,
                            std::io::Error::new(std::io::ErrorKind::Other, e)
                        )
                    })?;

                Ok(Some(Snapshot {
                    meta,
                    snapshot: Box::new(Cursor::new(bytes)),
                }))
            }
            _ => Ok(None),
        }
    }
}
