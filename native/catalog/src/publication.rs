// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded groups of complete packages, with one durable atomic publication.
use super::*;

pub struct CachePackageUpdate<'a> {
    pub package: &'a PackageIndex,
    pub rows: &'a [CachedPart],
    pub unsupported: &'a [CachedUnsupported],
}
#[derive(Clone, Copy, Debug)]
pub struct CacheBatchLimits {
    pub max_packages: usize,
    pub max_parts: usize,
    pub max_serialized_metadata_bytes: usize,
}
impl Default for CacheBatchLimits {
    fn default() -> Self {
        Self {
            max_packages: 32,
            max_parts: 4096,
            max_serialized_metadata_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CacheBatchProgress {
    pub package_index: usize,
    pub packages: usize,
    pub parts_done: usize,
    pub parts_total: usize,
}
#[derive(Clone, Debug, Default)]
pub struct StoreBatchTimings {
    pub total_ms: f64,
    pub lease_and_source_before_ms: f64,
    pub begin_ms: f64,
    pub sql_rows_ms: f64,
    pub source_after_ms: f64,
    pub commit_ms: f64,
    pub packages: usize,
    pub parts: usize,
    pub grams: u64,
}
impl CatalogCache {
    /// All rows in a package are complete; the bounded batch becomes visible
    /// atomically only after every source passes revalidation and FULL commit.
    /// Source read leases remain held through commit. Cancellation/failure
    /// preserves all previously committed cache generations in the batch.
    pub fn store_packages_with_progress(
        &mut self,
        updates: &[CachePackageUpdate<'_>],
        limits: CacheBatchLimits,
        cancel: &AtomicBool,
        mut progress: impl FnMut(CacheBatchProgress),
    ) -> Result<StoreBatchTimings> {
        cancelled(cancel)?;
        let total = Instant::now();
        let mut timing = StoreBatchTimings {
            packages: updates.len(),
            ..Default::default()
        };
        if updates.is_empty() {
            return Ok(timing);
        }
        if updates.len() > limits.max_packages
            || limits.max_packages == 0
            || limits.max_parts == 0
            || limits.max_serialized_metadata_bytes == 0
        {
            return Err(Error::Invalid("cache publication batch exceeds configured package budget; no source/cache data changed".into()));
        }
        let mut paths = HashSet::new();
        let mut metadata_bytes = 0usize;
        let mut leases = Vec::with_capacity(updates.len());
        for update in updates {
            cancelled(cancel)?;
            validate_complete(update.package, update.rows, update.unsupported)?;
            if !paths.insert(path_key(&update.package.path)) {
                return Err(Error::Invalid(
                    "duplicate source path in one cache publication batch".into(),
                ));
            }
            timing.parts = timing
                .parts
                .checked_add(update.rows.len() + update.unsupported.len())
                .ok_or_else(|| Error::Invalid("cache batch record count overflow".into()))?;
            if timing.parts > limits.max_parts {
                return Err(Error::Invalid(
                    "cache publication batch exceeds configured record budget".into(),
                ));
            }
            for row in update.rows {
                cancelled(cancel)?;
                metadata_bytes = metadata_bytes
                    .checked_add(serde_json::to_vec(&row.casp)?.len())
                    .ok_or_else(|| Error::Invalid("cache batch metadata size overflow".into()))?;
                if metadata_bytes > limits.max_serialized_metadata_bytes {
                    return Err(Error::Invalid(
                        "cache publication batch exceeds configured metadata byte budget".into(),
                    ));
                }
            }
            for status in update.unsupported {
                metadata_bytes = metadata_bytes
                    .checked_add(status.message.len())
                    .ok_or_else(|| Error::Invalid("cache batch status size overflow".into()))?;
                if metadata_bytes > limits.max_serialized_metadata_bytes {
                    return Err(Error::Invalid(
                        "cache publication batch exceeds configured metadata byte budget".into(),
                    ));
                }
            }
            leases.push(SourceLease::acquire(&update.package.path)?);
            update.package.revalidate(cancel)?;
        }
        timing.lease_and_source_before_ms = total.elapsed().as_secs_f64() * 1000.;
        let mut stage = Instant::now();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        timing.begin_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        let guard = ProgressGuard::new(&tx, cancel);
        let mut completed = 0;
        for (package_index, update) in updates.iter().enumerate() {
            let prior = completed;
            timing.grams += write_package_rows(
                &tx,
                update.package,
                update.rows,
                update.unsupported,
                cancel,
                |done| {
                    progress(CacheBatchProgress {
                        package_index,
                        packages: updates.len(),
                        parts_done: prior + done,
                        parts_total: timing.parts,
                    })
                },
            )?;
            completed += update.rows.len() + update.unsupported.len();
        }
        timing.sql_rows_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        for update in updates {
            cancelled(cancel)?;
            update.package.revalidate(cancel)?;
        }
        timing.source_after_ms = stage.elapsed().as_secs_f64() * 1000.;
        cancelled(cancel)?;
        stage = Instant::now();
        drop(guard);
        sql(tx.commit(), cancel)?;
        timing.commit_ms = stage.elapsed().as_secs_f64() * 1000.;
        drop(leases);
        timing.total_ms = total.elapsed().as_secs_f64() * 1000.;
        Ok(timing)
    }
}
/// Shared with the original one-package API; storage/schema/search semantics
/// remain identical. Cached statements are bounded by rusqlite's cache capacity.
pub(super) fn write_package_rows(
    tx: &rusqlite::Transaction<'_>,
    package: &PackageIndex,
    rows: &[CachedPart],
    unsupported: &[CachedUnsupported],
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize),
) -> Result<u64> {
    let fp = package.fingerprint();
    let key = path_key(&fp.path);
    let source_label = package
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let fingerprint = scope::fingerprint_signature(&fp);
    sql(tx.prepare_cached("INSERT INTO package(path_key,path_label,source_label,file_size,index_sha,modified_ns,created_ns,fingerprint_sha,expected_casp_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(path_key) DO UPDATE SET path_label=excluded.path_label,source_label=excluded.source_label,file_size=excluded.file_size,index_sha=excluded.index_sha,modified_ns=excluded.modified_ns,created_ns=excluded.created_ns,fingerprint_sha=excluded.fingerprint_sha,expected_casp_count=excluded.expected_casp_count")?.execute(params![key,package.path.to_string_lossy(),source_label,fp.file_size as i64,fp.index_sha256.as_slice(),time_key(fp.modified),time_key(fp.created),fingerprint.as_slice(),fp.casp_count as i64]),cancel)?;
    let package_id: i64 = tx
        .prepare_cached("SELECT id FROM package WHERE path_key=?1")?
        .query_row([path_key(&package.path)], |r| r.get(0))?;
    sql(
        tx.prepare_cached("DELETE FROM part WHERE package_id=?1")?
            .execute([package_id]),
        cancel,
    )?;
    sql(
        tx.prepare_cached("DELETE FROM unsupported WHERE package_id=?1")?
            .execute([package_id]),
        cancel,
    )?;
    let mut done = 0;
    let mut grams = 0;
    {
        let mut insert=tx.prepare_cached("INSERT INTO part(package_id,entry_index,key_type,key_group,key_instance,name,body_type,search_norm,payload_sha,casp_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)")?;
        let mut gram_insert = tx.prepare_cached("INSERT INTO gram(gram,part_id) VALUES(?1,?2)")?;
        for cached in rows {
            cancelled(cancel)?;
            let entry = &package.entries[cached.entry_index];
            let json = serde_json::to_string(&cached.casp)?;
            let normalized =
                format!("{} {} {}", cached.casp.name, source_label, entry.key).to_lowercase();
            sql(
                insert.execute(params![
                    package_id,
                    cached.entry_index as i64,
                    entry.key.type_id as i64,
                    entry.key.group_id as i64,
                    entry.key.instance_id.to_be_bytes().as_slice(),
                    cached.casp.name,
                    cached.casp.body_type as i64,
                    normalized,
                    cached.payload_sha256.as_slice(),
                    json
                ]),
                cancel,
            )?;
            let part_id = tx.last_insert_rowid();
            for gram in trigrams(&normalized) {
                cancelled(cancel)?;
                sql(gram_insert.execute(params![gram, part_id]), cancel)?;
                grams += 1;
            }
            done += 1;
            progress(done);
        }
        let mut insert = tx.prepare_cached(
            "INSERT INTO unsupported(package_id,entry_index,message) VALUES(?1,?2,?3)",
        )?;
        for status in unsupported {
            cancelled(cancel)?;
            sql(
                insert.execute(params![
                    package_id,
                    status.entry_index as i64,
                    status.message
                ]),
                cancel,
            )?;
            done += 1;
            progress(done);
        }
    }
    Ok(grams)
}
