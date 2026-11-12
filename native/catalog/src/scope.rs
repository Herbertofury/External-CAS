// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use external_cas_assets::dbpf::PackageFingerprint;
use sha2::{Digest, Sha256};

pub(super) fn fingerprint_signature(fp: &PackageFingerprint) -> [u8; 32] {
    signature_values(
        &path_key(&fp.path),
        fp.file_size,
        &fp.index_sha256,
        time_key(fp.modified).as_deref(),
        time_key(fp.created).as_deref(),
        fp.casp_count as u64,
    )
}
fn signature_values(
    path: &[u8],
    size: u64,
    index: &[u8],
    modified: Option<&str>,
    created: Option<&str>,
    casps: u64,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"external-cas-package-fingerprint-v2\0");
    hash.update((path.len() as u64).to_le_bytes());
    hash.update(path);
    hash.update(size.to_le_bytes());
    hash.update(index);
    hash.update(casps.to_le_bytes());
    for time in [modified, created] {
        if let Some(time) = time {
            hash.update([1]);
            hash.update((time.len() as u64).to_le_bytes());
            hash.update(time.as_bytes());
        } else {
            hash.update([0]);
        }
    }
    hash.finalize().into()
}
pub(super) fn migrate_v1(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute_batch("ALTER TABLE package ADD COLUMN fingerprint_sha BLOB; ALTER TABLE package ADD COLUMN expected_casp_count INTEGER NOT NULL DEFAULT -1;")?;
    let mut source=tx.prepare("SELECT p.id,p.path_key,p.file_size,p.index_sha,p.modified_ns,p.created_ns,(SELECT count(*) FROM part e WHERE e.package_id=p.id)+(SELECT count(*) FROM unsupported u WHERE u.package_id=p.id) FROM package p ORDER BY p.id")?;
    let mut rows = source.query([])?;
    let mut update =
        tx.prepare("UPDATE package SET fingerprint_sha=?1,expected_casp_count=?2 WHERE id=?3")?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let path: Vec<u8> = row.get(1)?;
        let size: i64 = row.get(2)?;
        let index: Vec<u8> = row.get(3)?;
        let modified: Option<String> = row.get(4)?;
        let created: Option<String> = row.get(5)?;
        let casps: i64 = row.get(6)?;
        if size < 0 || casps < 0 || index.len() != 32 {
            return Err(Error::Invalid(
                "invalid v1 package fingerprint; original cache preserved".into(),
            ));
        }
        let digest = signature_values(
            &path,
            size as u64,
            &index,
            modified.as_deref(),
            created.as_deref(),
            casps as u64,
        );
        update.execute(params![digest.as_slice(), casps, id])?;
    }
    Ok(())
}

impl CatalogCache {
    /// Bind one complete discovered source epoch. Membership is a join table,
    /// not an IN-list, so SQLite parameter counts do not cap library size.
    /// A failed/cancelled replacement rolls back to the previous valid scope.
    pub fn bind_scope(
        &mut self,
        scope_id: &str,
        expected: &[PackageFingerprint],
        cancel: &AtomicBool,
    ) -> Result<[u8; 32]> {
        self.bind_scope_with_progress(scope_id, expected, cancel, |_| {})
    }
    pub fn bind_scope_with_progress(
        &mut self,
        scope_id: &str,
        expected: &[PackageFingerprint],
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize),
    ) -> Result<[u8; 32]> {
        cancelled(cancel)?;
        if scope_id.is_empty() {
            return Err(Error::UnqualifiedScope("empty scope identity".into()));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let guard = ProgressGuard::new(&tx, cancel);
        let revision: i64 = sql(
            tx.query_row("SELECT revision FROM cache_metadata WHERE id=1", [], |r| {
                r.get(0)
            }),
            cancel,
        )?;
        sql(tx.execute("INSERT INTO scope(token,digest,package_count,bound_revision) VALUES(?1,zeroblob(32),?2,?3) ON CONFLICT(token) DO UPDATE SET digest=excluded.digest,package_count=excluded.package_count,bound_revision=excluded.bound_revision",params![scope_id,expected.len() as i64,revision]),cancel)?;
        let id: i64 = tx.query_row("SELECT id FROM scope WHERE token=?1", [scope_id], |r| {
            r.get(0)
        })?;
        sql(
            tx.execute("DELETE FROM scope_package WHERE scope_id=?1", [id]),
            cancel,
        )?;
        let mut digest = Sha256::new();
        digest.update(b"external-cas-discovery-scope-v1\0");
        digest.update((expected.len() as u64).to_le_bytes());
        {
            let mut lookup=tx.prepare("SELECT p.id,p.expected_casp_count,(SELECT count(*) FROM part e WHERE e.package_id=p.id)+(SELECT count(*) FROM unsupported u WHERE u.package_id=p.id) FROM package p WHERE p.path_key=?1 AND p.fingerprint_sha=?2")?;
            let mut insert=tx.prepare("INSERT INTO scope_package(scope_id,package_id,fingerprint_sha,expected_casp_count) VALUES(?1,?2,?3,?4)")?;
            for (i, fp) in expected.iter().enumerate() {
                cancelled(cancel)?;
                let fingerprint = fingerprint_signature(fp);
                let source = sql(
                    lookup
                        .query_row(params![path_key(&fp.path), fingerprint.as_slice()], |r| {
                            Ok((
                                r.get::<_, i64>(0)?,
                                r.get::<_, i64>(1)?,
                                r.get::<_, i64>(2)?,
                            ))
                        })
                        .optional(),
                    cancel,
                )?;
                let Some((package_id, count, actual)) = source else {
                    return Err(Error::UnqualifiedScope(format!(
                        "missing or stale cached package {}",
                        fp.path.file_name().unwrap_or_default().to_string_lossy()
                    )));
                };
                if count < 0 || count as usize != fp.casp_count || actual != count {
                    return Err(Error::UnqualifiedScope(
                        "cached CASP/status snapshot is incomplete".into(),
                    ));
                }
                sql(
                    insert.execute(params![id, package_id, fingerprint.as_slice(), count]),
                    cancel,
                )?;
                digest.update(fingerprint);
                progress(i + 1);
            }
        }
        let digest: [u8; 32] = digest.finalize().into();
        sql(
            tx.execute(
                "UPDATE scope SET digest=?1 WHERE id=?2",
                params![digest.as_slice(), id],
            ),
            cancel,
        )?;
        cancelled(cancel)?;
        drop(guard);
        sql(tx.commit(), cancel)?;
        Ok(digest)
    }
    pub fn search_scoped(
        &self,
        scope_id: &str,
        query: &str,
        body: Option<u32>,
        offset: usize,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<SearchHit>> {
        cancelled(cancel)?;
        let tx = self.connection.unchecked_transaction()?;
        let guard = ProgressGuard::new(&self.connection, cancel);
        let (id, count, bound_revision): (i64, i64, i64) = sql(
            tx.query_row(
                "SELECT id,package_count,bound_revision FROM scope WHERE token=?1",
                [scope_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional(),
            cancel,
        )?
        .ok_or_else(|| Error::UnqualifiedScope("scope is missing or was released".into()))?;
        let revision: i64 = sql(
            tx.query_row("SELECT revision FROM cache_metadata WHERE id=1", [], |r| {
                r.get(0)
            }),
            cancel,
        )?;
        if revision != bound_revision {
            let (members,mismatch):(i64,i64)=sql(tx.query_row("SELECT count(*),coalesce(sum(CASE WHEN p.id IS NULL OR p.fingerprint_sha IS NOT s.fingerprint_sha OR p.expected_casp_count<>s.expected_casp_count OR (SELECT count(*) FROM part e WHERE e.package_id=p.id)+(SELECT count(*) FROM unsupported u WHERE u.package_id=p.id)<>s.expected_casp_count THEN 1 ELSE 0 END),0) FROM scope_package s LEFT JOIN package p ON p.id=s.package_id WHERE s.scope_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))),cancel)?;
            if members != count || mismatch != 0 {
                return Err(Error::UnqualifiedScope(
                    "cached packages changed or became incomplete; bind a new discovery epoch"
                        .into(),
                ));
            }
        }
        // Qualification and results share this SQLite read snapshot. A concurrent
        // cache publisher cannot silently remove one member between the checks.
        let result = self.search_internal(Some(id), query, body, offset, limit, cancel)?;
        cancelled(cancel)?;
        drop(guard);
        sql(tx.commit(), cancel)?;
        Ok(result)
    }
    pub fn release_scope(&mut self, scope_id: &str, cancel: &AtomicBool) -> Result<()> {
        cancelled(cancel)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let guard = ProgressGuard::new(&tx, cancel);
        sql(
            tx.execute("DELETE FROM scope WHERE token=?1", [scope_id]),
            cancel,
        )?;
        cancelled(cancel)?;
        drop(guard);
        sql(tx.commit(), cancel)?;
        Ok(())
    }
    /// Forget stale cache metadata without touching the package itself. Existing
    /// search scopes then reject their missing member rather than truncate.
    pub fn invalidate_package(&mut self, path: &Path, cancel: &AtomicBool) -> Result<()> {
        cancelled(cancel)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let guard = ProgressGuard::new(&tx, cancel);
        sql(
            tx.execute("DELETE FROM package WHERE path_key=?1", [path_key(path)]),
            cancel,
        )?;
        cancelled(cancel)?;
        drop(guard);
        sql(tx.commit(), cancel)?;
        Ok(())
    }
}

pub(super) const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS cache_metadata(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL);
INSERT OR IGNORE INTO cache_metadata(id,revision) VALUES(1,0);
CREATE TABLE IF NOT EXISTS scope(id INTEGER PRIMARY KEY,token TEXT NOT NULL UNIQUE,digest BLOB NOT NULL,package_count INTEGER NOT NULL,bound_revision INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS scope_package(scope_id INTEGER NOT NULL REFERENCES scope(id) ON DELETE CASCADE,package_id INTEGER NOT NULL REFERENCES package(id) ON DELETE CASCADE,fingerprint_sha BLOB NOT NULL,expected_casp_count INTEGER NOT NULL,PRIMARY KEY(scope_id,package_id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS scope_source ON scope_package(package_id,scope_id);
CREATE TRIGGER IF NOT EXISTS package_insert_revision AFTER INSERT ON package BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS package_update_revision AFTER UPDATE ON package BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS package_delete_revision AFTER DELETE ON package BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS part_insert_revision AFTER INSERT ON part BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS part_update_revision AFTER UPDATE ON part BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS part_delete_revision AFTER DELETE ON part BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS unsupported_insert_revision AFTER INSERT ON unsupported BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS unsupported_update_revision AFTER UPDATE ON unsupported BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS unsupported_delete_revision AFTER DELETE ON unsupported BEGIN UPDATE cache_metadata SET revision=revision+1 WHERE id=1; END;
";
