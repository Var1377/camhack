use crate::game::GameEvent;
use anyhow::Result;
use openraft::storage::{LogState, Snapshot};
use openraft::{
    Entry, EntryPayload, LogId, RaftLogReader, RaftSnapshotBuilder, RaftStorage, SnapshotMeta,
    StorageError, StorageIOError, Vote,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
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
    pub events: Vec<GameEvent>,
    pub last_applied_log_index: u64,
}

/// Type config for OpenRaft
#[derive(Debug, Clone)]
pub struct GameRaftTypeConfig;

impl openraft::RaftTypeConfig for GameRaftTypeConfig {
    type D = GameEventRequest;
    type R = GameEventResponse;
    type NodeId = NodeId;
    type Node = ();
    type Entry = Entry<Self>;
    type SnapshotData = GameStateSnapshot;
    type AsyncRuntime = openraft::TokioRuntime;
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
}

/// Game state machine that stores all events
pub struct GameStateMachine {
    /// All game events in order
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
                events: Vec::new(),
                last_applied_log_index: 0,
            })),
            snapshot: Arc::new(RwLock::new(None)),
            snapshot_meta: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the state machine for reading game events
    pub fn state_machine(&self) -> Arc<RwLock<GameStateMachine>> {
        self.state_machine.clone()
    }
}

#[async_trait::async_trait]
impl RaftLogReader<GameRaftTypeConfig> for MemStorage {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + Send + Sync>(
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

#[async_trait::async_trait]
impl RaftSnapshotBuilder<GameRaftTypeConfig> for MemStorage {
    async fn build_snapshot(&mut self) -> Result<Snapshot<GameRaftTypeConfig>, StorageError<NodeId>> {
        let sm = self.state_machine.read().await;
        let snapshot_data = GameStateSnapshot {
            events: sm.events.clone(),
            last_applied_log_index: sm.last_applied_log_index,
        };

        let meta = SnapshotMeta {
            last_log_id: LogId::new(0, sm.last_applied_log_index),
            last_membership: Default::default(),
            snapshot_id: format!("snapshot-{}", sm.last_applied_log_index),
        };

        *self.snapshot.write().await = Some(snapshot_data.clone());
        *self.snapshot_meta.write().await = Some(meta.clone());

        Ok(Snapshot {
            meta,
            snapshot: Box::new(snapshot_data),
        })
    }
}

#[async_trait::async_trait]
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
    ) -> Result<(Option<LogId<NodeId>>, SnapshotMeta<NodeId, ()>), StorageError<NodeId>> {
        let sm = self.state_machine.read().await;
        let last_log_id = if sm.last_applied_log_index > 0 {
            Some(LogId::new(0, sm.last_applied_log_index))
        } else {
            None
        };

        let snapshot_meta = self.snapshot_meta.read().await.clone().unwrap_or_else(|| {
            SnapshotMeta {
                last_log_id: None,
                last_membership: Default::default(),
                snapshot_id: "empty".to_string(),
            }
        });

        Ok((last_log_id, snapshot_meta))
    }

    async fn apply_to_state_machine(
        &mut self,
        entries: &[Entry<GameRaftTypeConfig>],
    ) -> Result<Vec<GameEventResponse>, StorageError<NodeId>> {
        let mut sm = self.state_machine.write().await;
        let mut responses = Vec::new();

        for entry in entries {
            if let EntryPayload::Normal(request) = &entry.payload {
                sm.events.push(request.event.clone());
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

    async fn begin_receiving_snapshot(&mut self) -> Result<Box<GameStateSnapshot>, StorageError<NodeId>> {
        Ok(Box::new(GameStateSnapshot {
            events: Vec::new(),
            last_applied_log_index: 0,
        }))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, ()>,
        snapshot: Box<GameStateSnapshot>,
    ) -> Result<(), StorageError<NodeId>> {
        let mut sm = self.state_machine.write().await;
        sm.events = snapshot.events.clone();
        sm.last_applied_log_index = snapshot.last_applied_log_index;

        *self.snapshot.write().await = Some(*snapshot);
        *self.snapshot_meta.write().await = Some(meta.clone());

        Ok(())
    }

    async fn get_current_snapshot(&mut self) -> Result<Option<Snapshot<GameRaftTypeConfig>>, StorageError<NodeId>> {
        let snapshot = self.snapshot.read().await.clone();
        let meta = self.snapshot_meta.read().await.clone();

        match (snapshot, meta) {
            (Some(data), Some(meta)) => Ok(Some(Snapshot {
                meta,
                snapshot: Box::new(data),
            })),
            _ => Ok(None),
        }
    }
}

impl MemStorage {
    fn clone_storage(&self) -> Self {
        Self {
            vote: self.vote.clone(),
            log: self.log.clone(),
            state_machine: self.state_machine.clone(),
            snapshot: self.snapshot.clone(),
            snapshot_meta: self.snapshot_meta.clone(),
        }
    }
}
