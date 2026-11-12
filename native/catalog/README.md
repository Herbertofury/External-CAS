# Persistent native CASP catalog cache

This crate caches parsed CASP metadata in an explicit private app-data SQLite
database. It never changes a source package, save, Tray file or game process.
The cache is rebuildable; it is not the authority for asset editing or saving.

```rust,ignore
let mut cache = CatalogCache::open(absolute_app_data_path)?;
if let Some(cached) = cache.load_package(&package, cancel)? {
    // cached.rows: CachedPart { entry_index, payload_sha256, casp }
    // cached.unsupported: CachedUnsupported { entry_index, message }
} else {
    // Read/parse each nondeleted CASP, retaining its actual payload SHA256.
    cache.store_package(&package, &rows, &unsupported, cancel)?;
}
```

`PackageFingerprint` includes the exact source path, file length, observed
header/index SHA256, modified/created timestamps and active CASP count. A cache
hit requires the current observed fingerprint and complete recorded statuses.
`CasPart` serialization retains all parsed fields, ordered duplicate tags,
unsigned keys, role references and colors. Unsupported CASPs retain their exact
per-entry message. No missing or unsupported entry is silently truncated.

Each publication uses an immediate SQLite transaction. Rows, unsupported
statuses, source fingerprint and indexed search postings become visible
together. Cancellation, failed writes and stale source observations roll back
to the previous cache. On Windows a brief read-only source lease allows game
reads while excluding concurrent source writes/replacement during validation
and commit. Busy or incompatible source access fails explicitly. Other
platforms use before/after revalidation without claiming a mandatory write lock.

`store_package_with_progress` reports processed metadata/status counts, with a
separate successful return establishing transaction publication. A progress
callback is not a commit receipt. Schema 3 upgrades schema 1/2 transactionally,
preserving valid metadata. Unrelated databases and unknown schema versions are
refused. No arbitrary existing database is repurposed.

## Bounded complete-package batches

`store_packages_with_progress` accepts borrowed `CachePackageUpdate` records
and `CacheBatchLimits`. Defaults bound one pending group to32 packages,
4096 parsed/unsupported records and16MiB serialized CASP/status metadata. These
are staging limits, not library limits. Flush before exceeding them; a genuine
package larger than a batch uses the original complete-package API.

Every source is validated before publication and again before a single FULL
SQLite commit, with all Windows source leases retained throughout. Every
package remains complete, and the whole group publishes atomically. Failed or
cancelled batches preserve previous cache generations; callers must not blindly
retry a denied/stale batch. Flush the final group before binding search scope,
and treat a publication failure as an unqualified indexed epoch. Prepared
statements are reused with rusqlite's bounded statement cache. Source/payload
pinning and schema/search semantics are unchanged.

`StorePackageTimings` and `StoreBatchTimings` expose source validation, SQL and
commit wall times without private asset metadata. The `profile_metadata`
example accepts an explicit disposable cache path, private real-source package
manifest and optional batch size1..32. It measures a real-source component;
it does not establish an application speedup. Full native app cold/warm receipts
and targeted before/after data remain in the ignored private profiling folder.
`load_package_profiled` additionally separates source checks, SQLite row work
and JSON parsing. `set_page_cache_budget_kib` is an optional, verified,
per-connection SQLite target; the default is unchanged. It neither changes
durability/schema nor caps the library. The page cache is allocated lazily and
is not a process-memory ceiling. The app exposes this only as diagnostic
`EXTERNAL_CAS_CATALOG_CACHE_KIB`; an observed fast run alone does not qualify a
larger production default. [SQLite documents these cache and durability controls](https://www.sqlite.org/pragma.html).

## Qualified source scopes

```rust,ignore
let fingerprints = catalog.packages.iter().map(|p| p.fingerprint()).collect::<Vec<_>>();
let digest = cache.bind_scope("current-discovery-epoch", &fingerprints, cancel)?;
let hits = cache.search_scoped("current-discovery-epoch", query, body_type,
                               offset, limit, cancel)?;
```

Bind only after the caller establishes complete source discovery. A package
whose index could not be read is not represented by a fingerprint and must
keep the application's coverage unqualified. Cached per-entry unsupported
statuses do count as recorded coverage; they do not establish compatibility.

Scope membership is a join table, so libraries do not encounter a SQL `IN`
parameter-count cap. The scope digest includes every supplied package
fingerprint. It uses the supplied membership order for identity only; it does
not select game/mod load precedence. A failed or cancelled replacement retains
the previous scope. A separate scope token per discovery epoch prevents an
older valid scope from being mistaken for a failed new bind.

Queries reject missing, stale or incomplete cached scope members with
`Error::UnqualifiedScope`. Scope qualification and rows share one SQLite read
snapshot, so concurrent publication cannot silently omit a member between
qualification and retrieval. Cache revisions invalidate the fast qualification
path when source records, parts or statuses change. Duplicate full TGIs and
providers remain separate results. `release_scope` removes an obsolete scope;
`invalidate_package` removes only generated metadata and makes scopes referring
to it unqualified. Keep the previous application session until a new session
and scope have been accepted.

The unscoped `search` API searches historical cached sources and is intended
for explicit cache inspection. Use `search_scoped` for the current editor.

## Search and cancellation

Search text uses precisely Rust `str::to_lowercase` on name, source basename
and full TGI. Whitespace-separated query words must each occur as substrings.
Unicode character trigrams narrow candidates, followed by the exact Rust
substring check. One- and two-character words use a correct fallback. There
is no ASCII-only folding, accent removal, invented match total or hidden
query-length cap. The optional body-slot index remains available during
fallback. Results retain source path, ordinal, full TGI and payload pin for
joining back to current discovered providers.

SQLite's progress handler checks the supplied cancellation flag, including
long scans/sorts before the first result. Row loading and publication also
check cancellation. The narrowly scoped FFI callback is removed before its
borrowed flag can expire or a transaction rolls back. The private connection
is not exposed or shared concurrently.

## Verification and practical limits

`cargo test -p external-cas-catalog-cache --jobs 2` covers hits/reopen, exact
metadata/status retention, changed fingerprints, failed writes, cancellation
after partial insertion, independent readers, source-sharing conflicts,
Unicode/short search, duplicate providers, failed scope rebind, stale/missing/
partial scope rejection, schema migration, SQL interruption, and membership
above the historical 999-parameter threshold. The latter is a correctness
fixture, not a performance benchmark.

`examples/qualify.rs` accepts a private absolute database path followed by
explicit source package paths. `evidence/real-cache-qualification.json` records
the original cold/warm component run. `evidence/real-scoped-search-qualification.json`
records six real installed sources and 233 CASPs: full metadata/payload pins
survived reopen/upgrade, warm passes parsed zero CASPs, and sixteen scoped
query/body combinations exactly matched discovered metadata. These results
are component evidence, not populated app startup, input latency, frame-time
or representative large-library performance claims.

A scope qualifies one discovered source epoch against the cache. It does not
monitor the filesystem. Later external writes require a new discovery epoch
or watcher invalidation. Header/index/timestamp fingerprints cannot prove
unchanged payloads after a timestamp-preserving rewrite. Retain and revalidate
each selected CASP's payload SHA256 before preview/equip, as the native app's
source reader supports. Cache labels never authorize writing game state.

SQLite grows with observed source metadata and trigram postings. Releasing
old scopes removes their membership rows; package-cache eviction and large-
library disk/memory policy require separate actual-app profiling. Original
unknown/unsupported asset bytes remain in their read-only source packages.

## Licensing

GPL-3.0-or-later, following `external-cas-assets` and the recovered CASP format
implementation provenance. See `LICENSE-GPL-3.0.txt` and the assets crate's
provenance notes. `rusqlite` is MIT, bundled SQLite is public domain, `sha2`
and `serde_json` are MIT/Apache-2.0; test-only `tempfile` is MIT/Apache-2.0.
No source packages, creator assets, private manifests or game binaries are
included.
