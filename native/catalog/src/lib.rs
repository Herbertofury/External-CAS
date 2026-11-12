// SPDX-License-Identifier: GPL-3.0-or-later
//! Rebuildable app-owned CASP metadata cache. Never writes source packages.
use external_cas_assets::{CasPart, PackageIndex, ResourceKey, dbpf::CASP};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::{
    collections::HashSet,
    ffi::OsString,
    fmt,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const APPLICATION_ID: i32 = 0x45434153;
const SCHEMA_VERSION: i32 = 3;
mod publication;
mod scope;
pub use publication::{
    CacheBatchLimits, CacheBatchProgress, CachePackageUpdate, StoreBatchTimings,
};
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Asset(external_cas_assets::Error),
    Json(serde_json::Error),
    Invalid(String),
    UnqualifiedScope(String),
    Cancelled,
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl From<external_cas_assets::Error> for Error {
    fn from(e: external_cas_assets::Error) -> Self {
        if matches!(e, external_cas_assets::Error::Cancelled) {
            Self::Cancelled
        } else {
            Self::Asset(e)
        }
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Sqlite(e) => write!(f, "Cache database: {e}"),
            Self::Asset(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "Cached metadata: {e}"),
            Self::Invalid(e) => write!(f, "Cache: {e}"),
            Self::UnqualifiedScope(e) => write!(f, "Search scope is not qualified: {e}"),
            Self::Cancelled => write!(f, "Cache operation cancelled"),
        }
    }
}
impl std::error::Error for Error {}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn sql<T>(result: rusqlite::Result<T>, cancel: &AtomicBool) -> Result<T> {
    result.map_err(|e| {
        if cancel.load(Ordering::Relaxed) {
            Error::Cancelled
        } else {
            Error::Sqlite(e)
        }
    })
}

#[derive(Clone, Debug)]
pub struct CachedPart {
    pub entry_index: usize,
    pub payload_sha256: [u8; 32],
    pub casp: CasPart,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedUnsupported {
    pub entry_index: usize,
    pub message: String,
}
#[derive(Clone, Debug, Default)]
pub struct CachedPackage {
    pub rows: Vec<CachedPart>,
    pub unsupported: Vec<CachedUnsupported>,
}
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub package_path: PathBuf,
    pub entry_index: usize,
    pub key: ResourceKey,
    pub name: String,
    pub body_type: u32,
    pub source_label: String,
    pub payload_sha256: [u8; 32],
}
pub struct CatalogCache {
    connection: rusqlite::Connection,
    pub path: PathBuf,
}
/// Optional targeted wall timings for a real complete-package publication.
/// No source bytes, paths, metadata or payload hashes are included.
#[derive(Clone, Debug, Default)]
pub struct StorePackageTimings {
    pub total_ms: f64,
    pub validate_complete_ms: f64,
    pub lease_ms: f64,
    pub source_before_ms: f64,
    pub begin_ms: f64,
    pub sql_rows_ms: f64,
    pub source_after_ms: f64,
    pub commit_ms: f64,
    pub parts: usize,
    pub unsupported: usize,
    pub grams: u64,
}
#[derive(Clone, Debug, Default)]
pub struct LoadPackageTimings {
    pub total_ms: f64,
    pub lease_ms: f64,
    pub source_before_ms: f64,
    pub lookup_ms: f64,
    pub rows_ms: f64,
    pub json_parse_ms: f64,
    pub complete_validation_ms: f64,
    pub source_after_ms: f64,
    pub commit_ms: f64,
    pub parts: usize,
}

impl CatalogCache {
    /// Per-connection SQLite page-cache target, not a metadata/library limit.
    /// SQLite allocates lazily; this is not a process-memory ceiling.
    pub fn set_page_cache_budget_kib(&self, kib: u32) -> Result<()> {
        if kib == 0 || kib > i32::MAX as u32 {
            return Err(Error::Invalid(
                "SQLite page-cache budget is outside its supported positive range".into(),
            ));
        }
        let requested = -i64::from(kib);
        self.connection
            .pragma_update(None, "cache_size", requested)?;
        let actual: i64 = self
            .connection
            .pragma_query_value(None, "cache_size", |row| row.get(0))?;
        if actual != requested {
            return Err(Error::Invalid(
                "SQLite did not accept the requested page-cache target".into(),
            ));
        }
        Ok(())
    }
    /// The caller supplies a private app-data location or explicit test root.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(Error::Invalid("database path must be absolute".into()));
        }
        let parent = path
            .parent()
            .ok_or_else(|| Error::Invalid("database parent".into()))?;
        std::fs::create_dir_all(parent)?;
        let mut connection = rusqlite::Connection::open(path)?;
        let application_id: i32 =
            connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if application_id != APPLICATION_ID && (application_id != 0 || tables != 0) {
            return Err(Error::Invalid(
                "existing database belongs to another application".into(),
            ));
        }
        let version: i32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if !(0..=SCHEMA_VERSION).contains(&version) {
            return Err(Error::Invalid(format!(
                "unsupported schema version {version}; preserve and rebuild cache at another app-owned path"
            )));
        }
        connection.busy_timeout(std::time::Duration::from_millis(250))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        if version < SCHEMA_VERSION {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(SCHEMA)?;
            if version == 1 {
                scope::migrate_v1(&tx)?;
            }
            tx.execute_batch(scope::SCHEMA)?;
            if version == 2 {
                tx.execute(
                    "UPDATE cache_metadata SET revision=revision+1 WHERE id=1",
                    [],
                )?;
            }
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
        }
        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }
    pub fn load_package(
        &self,
        package: &PackageIndex,
        cancel: &AtomicBool,
    ) -> Result<Option<CachedPackage>> {
        self.load_package_profiled(package, cancel)
            .map(|(data, _)| data)
    }
    pub fn load_package_profiled(
        &self,
        package: &PackageIndex,
        cancel: &AtomicBool,
    ) -> Result<(Option<CachedPackage>, LoadPackageTimings)> {
        let total = Instant::now();
        let mut timing = LoadPackageTimings::default();
        let mut stage = Instant::now();
        cancelled(cancel)?;
        let _lease = SourceLease::acquire(&package.path)?;
        timing.lease_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        package.revalidate(cancel)?;
        timing.source_before_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        let fp = package.fingerprint();
        let key = path_key(&fp.path);
        let modified = time_key(fp.modified);
        let created = time_key(fp.created);
        let tx = self.connection.unchecked_transaction()?;
        let _guard = ProgressGuard::new(&self.connection, cancel);
        let fingerprint = scope::fingerprint_signature(&fp);
        let package_id:Option<i64>=sql(tx.query_row("SELECT id FROM package WHERE path_key=?1 AND file_size=?2 AND index_sha=?3 AND modified_ns IS ?4 AND created_ns IS ?5 AND fingerprint_sha=?6 AND expected_casp_count=?7",params![key,fp.file_size as i64,fp.index_sha256.as_slice(),modified,created,fingerprint.as_slice(),fp.casp_count as i64],|r|r.get(0)).optional(),cancel)?;
        timing.lookup_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        let Some(package_id) = package_id else {
            timing.total_ms = total.elapsed().as_secs_f64() * 1000.;
            return Ok((None, timing));
        };
        let mut result = CachedPackage::default();
        {
            let mut statement=tx.prepare("SELECT entry_index,payload_sha,casp_json,key_type,key_group,key_instance FROM part WHERE package_id=?1 ORDER BY entry_index")?;
            let mut rows = statement.query([package_id])?;
            while let Some(row) = sql(rows.next(), cancel)? {
                cancelled(cancel)?;
                let entry_index = usize_value(row.get(0)?)?;
                let payload_sha256 = fixed(row.get::<_, Vec<u8>>(1)?)?;
                let encoded: String = row.get(2)?;
                let parse = Instant::now();
                let casp: CasPart = serde_json::from_str(&encoded)?;
                timing.json_parse_ms += parse.elapsed().as_secs_f64() * 1000.;
                timing.parts += 1;
                // New source-derived compatibility fields must not silently
                // become zero when an older metadata cache is reopened.
                if casp.compatibility.is_none()
                    || casp.version >= 44 && casp.color_sliders.is_none()
                {
                    timing.total_ms = total.elapsed().as_secs_f64() * 1000.;
                    return Ok((None, timing));
                }
                let stored_key = ResourceKey::new(
                    u32_value(row.get(3)?)?,
                    u32_value(row.get(4)?)?,
                    u64::from_be_bytes(fixed(row.get::<_, Vec<u8>>(5)?)?),
                );
                let source = package
                    .entries
                    .get(entry_index)
                    .ok_or_else(|| Error::Invalid("cached entry outside source index".into()))?;
                if source.key != stored_key {
                    return Err(Error::Invalid(
                        "cached TGI differs from current locator".into(),
                    ));
                }
                result.rows.push(CachedPart {
                    entry_index,
                    payload_sha256,
                    casp,
                });
            }
        }
        {
            let mut statement=tx.prepare("SELECT entry_index,message FROM unsupported WHERE package_id=?1 ORDER BY entry_index")?;
            let mut rows = statement.query([package_id])?;
            while let Some(row) = sql(rows.next(), cancel)? {
                cancelled(cancel)?;
                let unsupported = CachedUnsupported {
                    entry_index: usize_value(row.get(0)?)?,
                    message: row.get(1)?,
                };
                // A historical parser mistook the trailing Unknown2 index
                // word for a committed flag. Reparse only these formerly
                // rejected locators now that zero is qualified. Do not delete
                // the old complete package until its replacement commits.
                if unsupported.message == "Unsupported asset: storage marker 0x0000"
                    && package.entries.get(unsupported.entry_index).is_some_and(|entry| {
                        entry.committed == 0 && [0, 0x5a42, 0xffff].contains(&entry.compression)
                    })
                {
                    timing.total_ms = total.elapsed().as_secs_f64() * 1000.;
                    return Ok((None, timing));
                }
                result.unsupported.push(unsupported);
            }
        }
        timing.rows_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        validate_complete(package, &result.rows, &result.unsupported)?;
        timing.complete_validation_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        package.revalidate(cancel)?;
        timing.source_after_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        cancelled(cancel)?;
        sql(tx.commit(), cancel)?;
        timing.commit_ms = stage.elapsed().as_secs_f64() * 1000.;
        timing.total_ms = total.elapsed().as_secs_f64() * 1000.;
        Ok((Some(result), timing))
    }
    pub fn store_package(
        &mut self,
        package: &PackageIndex,
        rows: &[CachedPart],
        unsupported: &[CachedUnsupported],
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.store_package_with_progress(package, rows, unsupported, cancel, |_| {})
    }
    pub fn store_package_with_progress(
        &mut self,
        package: &PackageIndex,
        rows: &[CachedPart],
        unsupported: &[CachedUnsupported],
        cancel: &AtomicBool,
        progress: impl FnMut(usize),
    ) -> Result<()> {
        self.store_package_profiled_with_progress(package, rows, unsupported, cancel, progress)
            .map(|_| ())
    }
    pub fn store_package_profiled_with_progress(
        &mut self,
        package: &PackageIndex,
        rows: &[CachedPart],
        unsupported: &[CachedUnsupported],
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize),
    ) -> Result<StorePackageTimings> {
        let total = Instant::now();
        let mut timings = StorePackageTimings {
            parts: rows.len(),
            unsupported: unsupported.len(),
            ..Default::default()
        };
        let mut stage = Instant::now();
        cancelled(cancel)?;
        validate_complete(package, rows, unsupported)?;
        timings.validate_complete_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        let _lease = SourceLease::acquire(&package.path)?;
        timings.lease_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        package.revalidate(cancel)?;
        timings.source_before_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        timings.begin_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        let _guard = ProgressGuard::new(&tx, cancel);
        timings.grams = publication::write_package_rows(
            &tx,
            package,
            rows,
            unsupported,
            cancel,
            &mut progress,
        )?;
        timings.sql_rows_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        cancelled(cancel)?;
        package.revalidate(cancel)?;
        timings.source_after_ms = stage.elapsed().as_secs_f64() * 1000.;
        stage = Instant::now();
        // The Windows read lease excludes source writes/replacements until commit.
        drop(_guard);
        sql(tx.commit(), cancel)?;
        timings.commit_ms = stage.elapsed().as_secs_f64() * 1000.;
        timings.total_ms = total.elapsed().as_secs_f64() * 1000.;
        Ok(timings)
    }
    /// Exact Rust lowercase/all-word substring semantics. Trigrams only narrow
    /// candidates; every result is checked with str::contains. Short queries
    /// fall back to a cancellable scan and keep the optional body-slot index.
    pub fn search(
        &self,
        query: &str,
        body: Option<u32>,
        offset: usize,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<SearchHit>> {
        let _guard = ProgressGuard::new(&self.connection, cancel);
        self.search_internal(None, query, body, offset, limit, cancel)
    }
    fn search_internal(
        &self,
        scope_id: Option<i64>,
        query: &str,
        body: Option<u32>,
        offset: usize,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<SearchHit>> {
        cancelled(cancel)?;
        if limit == 0 {
            return Ok(Vec::new());
        }
        let normalized = query.to_lowercase();
        let words = normalized.split_whitespace().collect::<Vec<_>>();
        let anchor = words
            .iter()
            .filter_map(|word| {
                let chars = word.chars().take(3).collect::<Vec<_>>();
                (chars.len() == 3).then(|| chars.into_iter().collect::<String>())
            })
            .next();
        let mut statement = String::from(
            "SELECT p.path_key,p.source_label,e.entry_index,e.key_type,e.key_group,e.key_instance,e.name,e.body_type,e.payload_sha,e.search_norm FROM part e JOIN package p ON p.id=e.package_id",
        );
        let mut values = Vec::<rusqlite::types::Value>::new();
        if let Some(scope_id) = scope_id {
            statement.push_str(" JOIN scope_package s ON s.package_id=p.id AND s.scope_id=?");
            values.push(scope_id.into());
        }
        if let Some(anchor) = anchor {
            statement.push_str(" JOIN gram g ON g.part_id=e.id AND g.gram=?");
            values.push(anchor.into());
        }
        if let Some(body) = body {
            statement.push_str(" WHERE e.body_type=?");
            values.push((body as i64).into());
        }
        statement.push_str(" ORDER BY e.name COLLATE BINARY,e.key_type,e.key_group,e.key_instance,p.path_key,e.entry_index");
        let mut prepared = self.connection.prepare(&statement)?;
        let mut rows = prepared.query(rusqlite::params_from_iter(values.iter()))?;
        let mut result = Vec::new();
        let mut matched = 0usize;
        while let Some(row) = sql(rows.next(), cancel)? {
            cancelled(cancel)?;
            let text: String = row.get(9)?;
            if !words.iter().all(|word| text.contains(word)) {
                continue;
            }
            if matched < offset {
                matched += 1;
                continue;
            }
            result.push(SearchHit {
                package_path: path_from_key(row.get(0)?)?,
                source_label: row.get(1)?,
                entry_index: usize_value(row.get(2)?)?,
                key: ResourceKey::new(
                    u32_value(row.get(3)?)?,
                    u32_value(row.get(4)?)?,
                    u64::from_be_bytes(fixed(row.get::<_, Vec<u8>>(5)?)?),
                ),
                name: row.get(6)?,
                body_type: u32_value(row.get(7)?)?,
                payload_sha256: fixed(row.get::<_, Vec<u8>>(8)?)?,
            });
            if result.len() == limit {
                break;
            }
        }
        Ok(result)
    }
}

fn validate_complete(
    package: &PackageIndex,
    rows: &[CachedPart],
    unsupported: &[CachedUnsupported],
) -> Result<()> {
    let mut seen = HashSet::new();
    for index in rows
        .iter()
        .map(|r| r.entry_index)
        .chain(unsupported.iter().map(|r| r.entry_index))
    {
        if !seen.insert(index) {
            return Err(Error::Invalid("duplicate cached source ordinal".into()));
        }
        let entry = package
            .entries
            .get(index)
            .ok_or_else(|| Error::Invalid("source ordinal outside package".into()))?;
        if entry.key.type_id != CASP || entry.deleted() {
            return Err(Error::Invalid("cached row is not an active CASP".into()));
        }
    }
    if package
        .entries
        .iter()
        .filter(|e| e.key.type_id == CASP && !e.deleted())
        .count()
        != seen.len()
    {
        return Err(Error::Invalid("package CASP snapshot is incomplete".into()));
    }
    Ok(())
}
fn fixed<const N: usize>(bytes: Vec<u8>) -> Result<[u8; N]> {
    bytes
        .try_into()
        .map_err(|_| Error::Invalid("invalid stored hash/key length".into()))
}
fn usize_value(n: i64) -> Result<usize> {
    usize::try_from(n).map_err(|_| Error::Invalid("negative or oversized source ordinal".into()))
}
fn u32_value(n: i64) -> Result<u32> {
    u32::try_from(n).map_err(|_| Error::Invalid("invalid unsigned cached field".into()))
}
fn time_key(time: Option<SystemTime>) -> Option<String> {
    time.map(|t| match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos().to_string(),
        Err(e) => format!("-{}", e.duration().as_nanos()),
    })
}
fn trigrams(text: &str) -> HashSet<String> {
    let chars = text.chars().collect::<Vec<_>>();
    chars.windows(3).map(|s| s.iter().collect()).collect()
}

#[cfg(windows)]
fn path_key(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
#[cfg(windows)]
fn path_from_key(bytes: Vec<u8>) -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    if bytes.len() % 2 != 0 {
        return Err(Error::Invalid("cached Windows path encoding".into()));
    }
    let words = bytes
        .chunks_exact(2)
        .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
        .collect::<Vec<_>>();
    Ok(OsString::from_wide(&words).into())
}
#[cfg(unix)]
fn path_key(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}
#[cfg(unix)]
fn path_from_key(bytes: Vec<u8>) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(bytes).into())
}

struct SourceLease {
    _file: File,
}
impl SourceLease {
    fn acquire(path: &Path) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        Ok(Self {
            _file: options.open(path)?,
        })
    }
}
/// This cache owns its connection and never exposes it. SQLite invokes the
/// callback synchronously while these methods borrow a live AtomicBool. The
/// guard removes the callback before either borrow can end or rollback runs.
struct ProgressGuard<'a> {
    handle: *mut rusqlite::ffi::sqlite3,
    _cancel: &'a AtomicBool,
}
impl<'a> ProgressGuard<'a> {
    fn new(connection: &rusqlite::Connection, cancel: &'a AtomicBool) -> Self {
        unsafe extern "C" fn callback(context: *mut std::ffi::c_void) -> std::ffi::c_int {
            // SAFETY: ProgressGuard keeps the supplied reference alive until removal.
            let cancel = unsafe { &*(context as *const AtomicBool) };
            i32::from(cancel.load(Ordering::Relaxed))
        }
        // SAFETY: the private Connection is borrowed for this synchronous operation;
        // Connection is not Sync, and no other callbacks are installed by the cache.
        let handle = unsafe { connection.handle() };
        unsafe {
            rusqlite::ffi::sqlite3_progress_handler(
                handle,
                1000,
                Some(callback),
                cancel as *const AtomicBool as *mut std::ffi::c_void,
            )
        };
        Self {
            handle,
            _cancel: cancel,
        }
    }
}
impl Drop for ProgressGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            rusqlite::ffi::sqlite3_progress_handler(self.handle, 0, None, std::ptr::null_mut())
        }
    }
}

const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS package(id INTEGER PRIMARY KEY,path_key BLOB NOT NULL UNIQUE,path_label TEXT NOT NULL,source_label TEXT NOT NULL,file_size INTEGER NOT NULL,index_sha BLOB NOT NULL,modified_ns TEXT,created_ns TEXT,fingerprint_sha BLOB,expected_casp_count INTEGER NOT NULL DEFAULT -1);
CREATE TABLE IF NOT EXISTS part(id INTEGER PRIMARY KEY,package_id INTEGER NOT NULL REFERENCES package(id) ON DELETE CASCADE,entry_index INTEGER NOT NULL,key_type INTEGER NOT NULL,key_group INTEGER NOT NULL,key_instance BLOB NOT NULL,name TEXT NOT NULL,body_type INTEGER NOT NULL,search_norm TEXT NOT NULL,payload_sha BLOB NOT NULL,casp_json TEXT NOT NULL,UNIQUE(package_id,entry_index));
CREATE INDEX IF NOT EXISTS part_body ON part(body_type,name);
CREATE INDEX IF NOT EXISTS part_name ON part(name,key_type,key_group,key_instance);
CREATE TABLE IF NOT EXISTS gram(gram TEXT NOT NULL,part_id INTEGER NOT NULL REFERENCES part(id) ON DELETE CASCADE,PRIMARY KEY(gram,part_id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS gram_part ON gram(part_id);
CREATE TABLE IF NOT EXISTS unsupported(package_id INTEGER NOT NULL REFERENCES package(id) ON DELETE CASCADE,entry_index INTEGER NOT NULL,message TEXT NOT NULL,PRIMARY KEY(package_id,entry_index)) WITHOUT ROWID;
";

#[cfg(test)]
mod tests;
