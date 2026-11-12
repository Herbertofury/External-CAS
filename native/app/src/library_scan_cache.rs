//! Bounded cache publication for discovery. No game/source writes.
use super::*;
use external_cas_catalog_cache::{
    CacheBatchLimits, CachePackageUpdate, LoadPackageTimings, StoreBatchTimings,
    StorePackageTimings,
};

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct CacheScanStats {
    pub page_cache_target_kib: Option<u32>,
    pub index_discovery_ms: f64,
    pub cache_load_ms: f64,
    pub cache_load_lease_ms: f64,
    pub cache_load_source_before_ms: f64,
    pub cache_load_lookup_ms: f64,
    pub cache_load_rows_ms: f64,
    pub cache_load_json_parse_ms: f64,
    pub cache_load_complete_validation_ms: f64,
    pub cache_load_source_after_ms: f64,
    pub metadata_decode_ms: f64,
    pub item_assembly_ms: f64,
    pub scope_bind_ms: f64,
    pub cache_publication_ms: f64,
    pub cache_sql_rows_ms: f64,
    pub cache_commit_ms: f64,
    pub cache_source_validation_ms: f64,
    pub batch_commits: usize,
    pub individual_oversized_commits: usize,
    pub published_packages: usize,
    pub publication_failures: usize,
    pub source_packages_failed: usize,
    pub queued_peak_packages: usize,
    pub queued_peak_records: usize,
    pub queued_peak_serialized_bytes: usize,
}
struct Pending {
    package_index: usize,
    metadata: CachedPackage,
}
pub(super) struct Publisher {
    pending: Vec<Pending>,
    records: usize,
    bytes: usize,
    pub stats: CacheScanStats,
    limits: CacheBatchLimits,
}
impl Publisher {
    pub fn add_load(&mut self, t: LoadPackageTimings) {
        self.stats.cache_load_lease_ms += t.lease_ms;
        self.stats.cache_load_source_before_ms += t.source_before_ms;
        self.stats.cache_load_lookup_ms += t.lookup_ms;
        self.stats.cache_load_rows_ms += t.rows_ms;
        self.stats.cache_load_json_parse_ms += t.json_parse_ms;
        self.stats.cache_load_complete_validation_ms += t.complete_validation_ms;
        self.stats.cache_load_source_after_ms += t.source_after_ms;
    }
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            records: 0,
            bytes: 0,
            stats: CacheScanStats::default(),
            limits: CacheBatchLimits::default(),
        }
    }
    pub fn enqueue(
        &mut self,
        cache: &mut CatalogCache,
        catalog: &Catalog,
        package_index: usize,
        metadata: &CachedPackage,
        cancel: &AtomicBool,
        generation: u64,
        tx: &Sender<Event>,
        last: &mut Instant,
        issues: &mut Vec<String>,
    ) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Discovery cancelled".into());
        }
        let records = metadata.rows.len() + metadata.unsupported.len();
        let mut bytes = 0usize;
        for row in &metadata.rows {
            if cancel.load(Ordering::Relaxed) {
                return Err("Discovery cancelled".into());
            }
            let json = match serde_json::to_vec(&row.casp) {
                Ok(json) => json,
                Err(error) => {
                    self.stats.publication_failures += 1;
                    issues.push(format!("Cache metadata encoding failed: {error}"));
                    return Ok(());
                }
            };
            bytes = bytes.saturating_add(json.len());
        }
        bytes = metadata.unsupported.iter().fold(bytes, |sum, status| {
            sum.saturating_add(status.message.len())
        });
        let oversized =
            records > self.limits.max_parts || bytes > self.limits.max_serialized_metadata_bytes;
        if oversized
            || self.pending.len() >= self.limits.max_packages
            || self.records.saturating_add(records) > self.limits.max_parts
            || self.bytes.saturating_add(bytes) > self.limits.max_serialized_metadata_bytes
        {
            self.flush(cache, catalog, cancel, generation, tx, last, issues)?;
        }
        if oversized {
            let package = &catalog.packages[package_index];
            match cache.store_package_profiled_with_progress(
                package,
                &metadata.rows,
                &metadata.unsupported,
                cancel,
                |done| {
                    notice(generation, tx, last, || {
                        format!("Caching complete package · {done} records")
                    })
                },
            ) {
                Ok(timing) => {
                    self.add_single(timing);
                    self.stats.individual_oversized_commits += 1;
                    self.stats.published_packages += 1;
                }
                Err(error) => {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Discovery cancelled".into());
                    }
                    self.stats.publication_failures += 1;
                    issues.push(format!("{}: {error}", package.path.display()));
                }
            }
            return Ok(());
        }
        self.pending.push(Pending {
            package_index,
            metadata: metadata.clone(),
        });
        self.records += records;
        self.bytes += bytes;
        self.stats.queued_peak_packages = self.stats.queued_peak_packages.max(self.pending.len());
        self.stats.queued_peak_records = self.stats.queued_peak_records.max(self.records);
        self.stats.queued_peak_serialized_bytes =
            self.stats.queued_peak_serialized_bytes.max(self.bytes);
        Ok(())
    }
    pub fn flush(
        &mut self,
        cache: &mut CatalogCache,
        catalog: &Catalog,
        cancel: &AtomicBool,
        generation: u64,
        tx: &Sender<Event>,
        last: &mut Instant,
        issues: &mut Vec<String>,
    ) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        self.records = 0;
        self.bytes = 0;
        let updates = pending
            .iter()
            .map(|p| CachePackageUpdate {
                package: &catalog.packages[p.package_index],
                rows: &p.metadata.rows,
                unsupported: &p.metadata.unsupported,
            })
            .collect::<Vec<_>>();
        match cache.store_packages_with_progress(&updates, self.limits, cancel, |p| {
            notice(generation, tx, last, || {
                format!(
                    "Caching appearance metadata · {} / {} records · {} complete sources",
                    p.parts_done, p.parts_total, p.packages
                )
            })
        }) {
            Ok(timing) => {
                self.stats.published_packages += timing.packages;
                self.stats.batch_commits += 1;
                self.add_batch(timing);
            }
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Discovery cancelled".into());
                }
                self.stats.publication_failures += 1;
                issues.push(format!(
                    "Cache batch of {} sources was not published: {error}",
                    updates.len()
                ));
            }
        }
        Ok(())
    }
    fn add_batch(&mut self, t: StoreBatchTimings) {
        self.stats.cache_publication_ms += t.total_ms;
        self.stats.cache_sql_rows_ms += t.sql_rows_ms;
        self.stats.cache_commit_ms += t.commit_ms;
        self.stats.cache_source_validation_ms += t.lease_and_source_before_ms + t.source_after_ms;
    }
    fn add_single(&mut self, t: StorePackageTimings) {
        self.stats.cache_publication_ms += t.total_ms;
        self.stats.cache_sql_rows_ms += t.sql_rows_ms;
        self.stats.cache_commit_ms += t.commit_ms;
        self.stats.cache_source_validation_ms +=
            t.lease_ms + t.source_before_ms + t.source_after_ms;
    }
}
fn notice(
    generation: u64,
    tx: &Sender<Event>,
    last: &mut Instant,
    message: impl FnOnce() -> String,
) {
    if last.elapsed().as_millis() >= 100 {
        let _ = tx.send(Event::Progress {
            generation,
            message: message(),
        });
        *last = Instant::now();
    }
}
