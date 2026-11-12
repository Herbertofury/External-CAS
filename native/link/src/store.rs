// SPDX-License-Identifier: GPL-3.0-or-later
//! Atomic app-owned checkpoint storage. Never opens a game/profile/save path.
use crate::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
const APP_ID: i32 = 0x45434c4b;
const VERSION: i32 = 1;
const DOMAIN: &[u8] = b"External CAS link checkpoint v1\0";
#[derive(Clone, Debug)]
pub struct StoreLimits {
    pub max_row_bytes: usize,
    pub max_total_bytes: u64,
}
impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            max_row_bytes: 256 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CheckpointReceipt {
    pub path: PathBuf,
    pub generation: u64,
    pub sha256: Digest32,
}
pub struct LoadedCheckpoint {
    pub controller: LinkController,
    pub receipt: CheckpointReceipt,
}
pub struct LinkStore {
    connection: Connection,
    path: PathBuf,
    limits: StoreLimits,
    qualified_generation: Cell<Option<u64>>,
}
impl LinkStore {
    pub fn open(path: impl AsRef<Path>, limits: StoreLimits) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_absolute() || limits.max_row_bytes == 0 || limits.max_total_bytes == 0 {
            return Err(LinkError::Invalid(
                "Explicit absolute app-owned link store path and budgets required".into(),
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let app_id: i32 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if app_id != APP_ID && (app_id != 0 || tables != 0) {
            return Err(LinkError::Invalid(
                "Foreign SQLite file preserved; select an app-owned store".into(),
            ));
        }
        if version > VERSION || version < 0 || app_id == APP_ID && version != VERSION {
            return Err(LinkError::Unsupported(
                "Link store schema is unknown; original database preserved".into(),
            ));
        }
        if app_id == 0 {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE checkpoint(singleton INTEGER PRIMARY KEY CHECK(singleton=1),generation INTEGER NOT NULL,sha256 BLOB NOT NULL,manifest_json BLOB NOT NULL);CREATE TABLE sim_context(context_key BLOB PRIMARY KEY,identity_json BLOB NOT NULL,data_json BLOB NOT NULL);CREATE TABLE operation(operation_id BLOB PRIMARY KEY,data_json BLOB NOT NULL);")?;
            tx.pragma_update(None, "application_id", APP_ID)?;
            tx.pragma_update(None, "user_version", VERSION)?;
            tx.commit()?;
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self {
            connection,
            path: path.to_path_buf(),
            limits,
            qualified_generation: Cell::new((app_id == 0).then_some(0)),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self, controller_limits: LinkLimits) -> Result<Option<LoadedCheckpoint>> {
        self.load_cancellable(controller_limits, &AtomicBool::new(false))
    }
    pub fn load_cancellable(
        &self,
        controller_limits: LinkLimits,
        cancel: &AtomicBool,
    ) -> Result<Option<LoadedCheckpoint>> {
        cancelled(cancel)?;
        self.qualified_generation.set(None);
        let tx = self.connection.unchecked_transaction()?;
        let metadata: Option<(i64, Vec<u8>, i64)> = tx
            .query_row(
                "SELECT generation,sha256,length(manifest_json) FROM checkpoint WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((generation, stored_hash, manifest_length)) = metadata else {
            let orphaned: i64 = tx.query_row(
                "SELECT (SELECT count(*) FROM sim_context)+(SELECT count(*) FROM operation)",
                [],
                |r| r.get(0),
            )?;
            if orphaned != 0 {
                return Err(LinkError::Invalid(
                    "Orphaned context/operation rows preserved; no valid checkpoint manifest"
                        .into(),
                ));
            }
            self.qualified_generation.set(Some(0));
            return Ok(None);
        };
        if generation <= 0 || stored_hash.len() != 32 {
            return Err(LinkError::Invalid(
                "Link checkpoint generation/hash extent".into(),
            ));
        }
        self.row_bound(sql_length(manifest_length)?)?;
        self.total_bound(sql_length(manifest_length)? as u64)?;
        let manifest: Vec<u8> = tx.query_row(
            "SELECT manifest_json FROM checkpoint WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let mut digest = Sha256::new();
        digest.update(DOMAIN);
        digest.update((generation as u64).to_le_bytes());
        hash_row(&mut digest, &[], &manifest);
        let mut retained = manifest.len() as u64;
        let mut saved: PersistedContexts = serde_json::from_slice(&manifest)?;
        if !saved.sims.is_empty() || !saved.operations.is_empty() {
            return Err(LinkError::Invalid(
                "Link checkpoint manifest duplicates row data".into(),
            ));
        }
        {
            let mut statement = tx.prepare(
                "SELECT context_key,identity_json,data_json,length(context_key),length(identity_json),length(data_json) FROM sim_context ORDER BY context_key",
            )?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                cancelled(cancel)?;
                let key_length = sql_length(row.get(3)?)?;
                let identity_length = sql_length(row.get(4)?)?;
                let data_length = sql_length(row.get(5)?)?;
                if key_length != 32 || identity_length > 4096 {
                    return Err(LinkError::Invalid("Checkpoint identity extent".into()));
                }
                self.row_bound(data_length)?;
                self.total_bound(retained.saturating_add(data_length as u64))?;
                let key: Vec<u8> = row.get(0)?;
                let identity: Vec<u8> = row.get(1)?;
                let data: Vec<u8> = row.get(2)?;
                self.row_bound(data.len())?;
                retained = retained
                    .checked_add(data.len() as u64)
                    .ok_or_else(|| LinkError::Budget("Checkpoint byte overflow".into()))?;
                self.total_bound(retained)?;
                if key.len() != 32 || key != Sha256::digest(&identity).as_slice() {
                    return Err(LinkError::Invalid(
                        "Link checkpoint context identity pin".into(),
                    ));
                }
                let identity: SimIdentity = serde_json::from_slice(&identity)?;
                let context: SimContext = serde_json::from_slice(&data)?;
                hash_row(&mut digest, &key, &data);
                saved.sims.push((identity, context));
                if saved.sims.len() > controller_limits.max_contexts {
                    return Err(LinkError::Budget(
                        "Checkpoint context count; all rows preserved".into(),
                    ));
                }
            }
        }
        {
            let mut statement =
                tx.prepare("SELECT operation_id,data_json,length(operation_id),length(data_json) FROM operation ORDER BY operation_id")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                cancelled(cancel)?;
                let id_length = sql_length(row.get(2)?)?;
                let data_length = sql_length(row.get(3)?)?;
                if id_length != 16 {
                    return Err(LinkError::Invalid("Checkpoint operation ID extent".into()));
                }
                self.row_bound(data_length)?;
                self.total_bound(retained.saturating_add(data_length as u64))?;
                let id: Vec<u8> = row.get(0)?;
                let data: Vec<u8> = row.get(1)?;
                self.row_bound(data.len())?;
                retained = retained
                    .checked_add(data.len() as u64)
                    .ok_or_else(|| LinkError::Budget("Checkpoint byte overflow".into()))?;
                self.total_bound(retained)?;
                let operation: ApplyTransaction = serde_json::from_slice(&data)?;
                if id != operation.operation_id {
                    return Err(LinkError::Invalid(
                        "Link checkpoint operation ID differs".into(),
                    ));
                }
                hash_row(&mut digest, &id, &data);
                saved.operations.push(operation);
                if saved.operations.len() > controller_limits.max_history {
                    return Err(LinkError::Budget(
                        "Checkpoint operation count; all rows preserved".into(),
                    ));
                }
            }
        }
        let actual: Digest32 = digest.finalize().into();
        if stored_hash != actual {
            return Err(LinkError::Invalid(
                "Link checkpoint digest differs; previous records preserved".into(),
            ));
        }
        let controller = LinkController::restore_contexts(saved, controller_limits)?;
        cancelled(cancel)?;
        tx.commit()?;
        self.qualified_generation.set(Some(generation as u64));
        Ok(Some(LoadedCheckpoint {
            controller,
            receipt: CheckpointReceipt {
                path: self.path.clone(),
                generation: generation as u64,
                sha256: actual,
            },
        }))
    }
    pub fn checkpoint(
        &mut self,
        controller: &LinkController,
        expected_generation: u64,
    ) -> Result<CheckpointReceipt> {
        self.checkpoint_cancellable(controller, expected_generation, &AtomicBool::new(false))
    }
    pub fn checkpoint_cancellable(
        &mut self,
        controller: &LinkController,
        expected_generation: u64,
        cancel: &AtomicBool,
    ) -> Result<CheckpointReceipt> {
        cancelled(cancel)?;
        if self.qualified_generation.get() != Some(expected_generation) {
            return Err(LinkError::Stale(
                "Load and qualify the current checkpoint before replacing it".into(),
            ));
        }
        let mut saved = controller.export_contexts();
        let mut contexts = Vec::with_capacity(saved.sims.len());
        let mut operations = Vec::with_capacity(saved.operations.len());
        let mut total = 0u64;
        LinkController::restore_contexts(saved.clone(), controller.limits.clone())?;
        for (identity, context) in saved.sims.drain(..) {
            let identity = bounded_json(&identity, self.limits.max_row_bytes, cancel)?;
            let key: Digest32 = Sha256::digest(&identity).into();
            let data = bounded_json(&context, self.limits.max_row_bytes, cancel)?;
            total = total.saturating_add(data.len() as u64);
            self.total_bound(total)?;
            contexts.push((key, identity, data));
        }
        contexts.sort_by_key(|r| r.0);
        for operation in saved.operations.drain(..) {
            let data = bounded_json(&operation, self.limits.max_row_bytes, cancel)?;
            total = total.saturating_add(data.len() as u64);
            self.total_bound(total)?;
            operations.push((operation.operation_id, data));
        }
        operations.sort_by_key(|r| r.0);
        let manifest = bounded_json(&saved, self.limits.max_row_bytes, cancel)?;
        total = total.saturating_add(manifest.len() as u64);
        self.total_bound(total)?;
        let generation = expected_generation
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| LinkError::Invalid("Checkpoint generation exhausted".into()))?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT generation FROM checkpoint WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if current.unwrap_or(0) != expected_generation as i64 {
            return Err(LinkError::Stale(
                "Another app instance published a newer link checkpoint; nothing overwritten"
                    .into(),
            ));
        }
        let mut digest = Sha256::new();
        digest.update(DOMAIN);
        digest.update(generation.to_le_bytes());
        hash_row(&mut digest, &[], &manifest);
        tx.execute("DELETE FROM sim_context", [])?;
        tx.execute("DELETE FROM operation", [])?;
        for (key, identity, data) in contexts {
            cancelled(cancel)?;
            hash_row(&mut digest, &key, &data);
            tx.execute(
                "INSERT INTO sim_context(context_key,identity_json,data_json) VALUES(?1,?2,?3)",
                params![key.as_slice(), identity, data],
            )?;
        }
        for (id, data) in operations {
            cancelled(cancel)?;
            hash_row(&mut digest, &id, &data);
            tx.execute(
                "INSERT INTO operation(operation_id,data_json) VALUES(?1,?2)",
                params![id.as_slice(), data],
            )?;
        }
        let sha256: Digest32 = digest.finalize().into();
        tx.execute("INSERT INTO checkpoint(singleton,generation,sha256,manifest_json) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET generation=excluded.generation,sha256=excluded.sha256,manifest_json=excluded.manifest_json",params![generation as i64,sha256.as_slice(),manifest])?;
        cancelled(cancel)?;
        tx.commit()?;
        self.qualified_generation.set(Some(generation));
        Ok(CheckpointReceipt {
            path: self.path.clone(),
            generation,
            sha256,
        })
    }
    fn row_bound(&self, bytes: usize) -> Result<()> {
        if bytes > self.limits.max_row_bytes {
            Err(LinkError::Budget(
                "Checkpoint row; original data preserved".into(),
            ))
        } else {
            Ok(())
        }
    }
    fn total_bound(&self, bytes: u64) -> Result<()> {
        if bytes > self.limits.max_total_bytes {
            Err(LinkError::Budget(
                "Checkpoint total; original data preserved".into(),
            ))
        } else {
            Ok(())
        }
    }
}
fn hash_row(hash: &mut Sha256, key: &[u8], data: &[u8]) {
    hash.update((key.len() as u64).to_le_bytes());
    hash.update(key);
    hash.update((data.len() as u64).to_le_bytes());
    hash.update(data);
}
fn sql_length(value: i64) -> Result<usize> {
    usize::try_from(value)
        .map_err(|_| LinkError::Invalid("Negative/overflowed SQLite extent".into()))
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(LinkError::Cancelled)
    } else {
        Ok(())
    }
}
fn bounded_json(value: &impl Serialize, limit: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    struct Buffer<'a> {
        bytes: Vec<u8>,
        limit: usize,
        overflow: bool,
        cancel: &'a AtomicBool,
    }
    impl Write for Buffer<'_> {
        fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::other("Checkpoint cancelled"));
            }
            if self
                .bytes
                .len()
                .checked_add(input.len())
                .is_none_or(|n| n > self.limit)
            {
                self.overflow = true;
                return Err(std::io::Error::other("Checkpoint bound"));
            }
            self.bytes.extend(input);
            Ok(input.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Buffer {
        bytes: Vec::new(),
        limit,
        overflow: false,
        cancel,
    };
    let encoded = serde_json::to_writer(&mut out, value);
    cancelled(cancel)?;
    if out.overflow {
        return Err(LinkError::Budget(
            "Serialized checkpoint row; original data preserved".into(),
        ));
    }
    encoded?;
    Ok(out.bytes)
}
