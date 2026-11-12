// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;
fn part(name: &str, body_type: u32) -> CasPart {
    serde_json::from_value(serde_json::json!({
        "version":52,"name":name,"sort_priority":1.5,"swatch_order":7,"outfit_id":0x80000000u32,"material_hash":123,"flags1":255,"flags2":128,"layer_id":65535,"compatibility":{"exclude_part_flags":[1,0,0xffffffffu64],"exclude_modifier_region_flags":0xffffffffu64,"opposite_gender_part":0xfedcba9876543210u64,"fallback_part":0,"lod_count":1},
        "tags":[[65535,0xffffffffu32],[1,1],[1,1]],"price":0xffffffffu32,"title_key":13,"description_key":14,"create_description_key":15,"texture_space":2,"body_type":body_type,"body_sub_type":19,
        "age":255,"gender":32,"age_gender_reserved":65535,"species":1,"pack_id":23,"pack_flags":128,"colors":[0xffffffffu32,0],"occult_mask":0xffffffffu32,"sort_layer":-5,"composition_method":3,
        "references":[{"type_id":CASP,"group_id":0x80000000u32,"instance_id":0xfedcba9876543210u64}],"roles":[{"role":"parent","table_index":0,"lod":null,"key":{"type_id":CASP,"group_id":0x80000000u32,"instance_id":0xfedcba9876543210u64}}],"diffuse":null,"normal":null,"specular":null,"lod0_meshes":[]
    })).unwrap()
}
#[test]
fn older_metadata_missing_compatibility_is_a_miss_without_destroying_previous_cache() {
    let temp = tempfile::tempdir().unwrap();
    let (package, rows) = source(&temp.path().join("original.package"), 1);
    let mut cache = CatalogCache::open(temp.path().join("cache.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&package, &rows, &[], &cancel).unwrap();
    let original: String = cache
        .connection
        .query_row("SELECT casp_json FROM part", [], |r| r.get(0))
        .unwrap();
    let mut older: serde_json::Value = serde_json::from_str(&original).unwrap();
    older.as_object_mut().unwrap().remove("compatibility");
    let older = serde_json::to_string(&older).unwrap();
    cache
        .connection
        .execute("UPDATE part SET casp_json=?1", [&older])
        .unwrap();
    assert!(cache.load_package(&package, &cancel).unwrap().is_none());
    let retained: String = cache
        .connection
        .query_row("SELECT casp_json FROM part", [], |r| r.get(0))
        .unwrap();
    assert_eq!(retained, older);
    cache.store_package(&package, &rows, &[], &cancel).unwrap();
    let current = cache.load_package(&package, &cancel).unwrap().unwrap();
    assert!(current.rows[0].casp.compatibility.is_some());
}
fn source(path: &Path, n: usize) -> (PackageIndex, Vec<CachedPart>) {
    let mut b = vec![0; 96];
    b[..4].copy_from_slice(b"DBPF");
    b[4..8].copy_from_slice(&2u32.to_le_bytes());
    b[8..12].copy_from_slice(&1u32.to_le_bytes());
    b[36..40].copy_from_slice(&(n as u32).to_le_bytes());
    let mut locators = Vec::new();
    let mut cached = Vec::new();
    for i in 0..n {
        let raw = format!("explicit synthetic cache boundary payload {i}").into_bytes();
        let at = b.len();
        b.extend(&raw);
        locators.push((at, raw.len()));
        cached.push(CachedPart {
            entry_index: i,
            payload_sha256: Sha256::digest(&raw).into(),
            casp: part(
                ["İSTANBUL Blue Hair", "Straße Dress", "ΣΙΓΜΑ gold accessory"][i % 3],
                if i % 2 == 0 { 2 } else { 5 },
            ),
        });
    }
    let index_at = b.len();
    let mut index = 0u32.to_le_bytes().to_vec();
    for (i, (offset, size)) in locators.iter().enumerate() {
        for x in [
            CASP,
            0x80000000,
            0xfedcba98,
            i as u32,
            *offset as u32,
            *size as u32,
            *size as u32,
        ] {
            index.extend(x.to_le_bytes());
        }
    }
    b[64..68].copy_from_slice(&(index_at as u32).to_le_bytes());
    b[44..48].copy_from_slice(&(index.len() as u32).to_le_bytes());
    b.extend(index);
    std::fs::write(path, b).unwrap();
    (PackageIndex::open(path).unwrap(), cached)
}
#[test]
fn cache_hit_roundtrip_unknown_fields_and_unsupported_statuses() {
    let root = tempfile::tempdir().unwrap();
    let (package, mut rows) = source(&root.path().join("CaseSensitive.package"), 3);
    let unsupported = vec![CachedUnsupported {
        entry_index: 2,
        message: "Unsupported future CASP v99\nRaw source retained".into(),
    }];
    rows.pop();
    let path = root.path().join("metadata.sqlite");
    let cancel = AtomicBool::new(false);
    let mut cache = CatalogCache::open(&path).unwrap();
    assert!(cache.load_package(&package, &cancel).unwrap().is_none());
    cache
        .store_package(&package, &rows, &unsupported, &cancel)
        .unwrap();
    drop(cache);
    let cache = CatalogCache::open(&path).unwrap();
    let loaded = cache.load_package(&package, &cancel).unwrap().unwrap();
    assert_eq!(loaded.unsupported, unsupported);
    assert_eq!(loaded.rows.len(), 2);
    for (a, b) in loaded.rows.iter().zip(&rows) {
        assert_eq!(a.entry_index, b.entry_index);
        assert_eq!(a.payload_sha256, b.payload_sha256);
        assert_eq!(
            serde_json::to_value(&a.casp).unwrap(),
            serde_json::to_value(&b.casp).unwrap()
        );
    }
}
#[test]
fn indexed_and_short_unicode_search_match_exact_rust_lowercase_substrings() {
    let root = tempfile::tempdir().unwrap();
    let (package, rows) = source(&root.path().join("Café_Source.package"), 9);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&package, &rows, &[], &cancel).unwrap();
    for query in [
        "",
        "i",
        "İS",
        "r",
        "ra",
        "straße",
        "STRASSE",
        "café",
        "i\u{307}",
        "blue hair",
        "ΣΙΓΜΑ",
        "gold accessory",
        "e.package",
        "FEDCBA98",
        "not present",
    ] {
        for body in [None, Some(2), Some(5)] {
            let lower = query.to_lowercase();
            let words = lower.split_whitespace().collect::<Vec<_>>();
            let source = package.path.file_name().unwrap().to_string_lossy();
            let expected = rows
                .iter()
                .filter(|r| body.is_none_or(|b| r.casp.body_type == b))
                .filter(|r| {
                    let text = format!(
                        "{} {} {}",
                        r.casp.name, source, package.entries[r.entry_index].key
                    )
                    .to_lowercase();
                    words.iter().all(|w| text.contains(w))
                })
                .count();
            let all = cache.search(query, body, 0, 100, &cancel).unwrap();
            assert_eq!(all.len(), expected, "query={query:?} body={body:?}");
            for hit in &all {
                assert_eq!(hit.key.instance_id >> 32, 0xfedcba98);
                assert_eq!(hit.package_path, package.path);
            }
            let page = cache.search(query, body, 1, 2, &cancel).unwrap();
            assert_eq!(
                page.iter().map(|r| r.entry_index).collect::<Vec<_>>(),
                all.iter()
                    .skip(1)
                    .take(2)
                    .map(|r| r.entry_index)
                    .collect::<Vec<_>>()
            );
        }
    }
}
#[test]
fn fingerprint_change_invalidates_without_deleting_previous_generation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.package");
    let (old, rows) = source(&path, 2);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&old, &rows, &[], &cancel).unwrap();
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"later extra source bytes").unwrap();
    drop(file);
    assert!(cache.load_package(&old, &cancel).is_err());
    let current = PackageIndex::open(&path).unwrap();
    assert!(cache.load_package(&current, &cancel).unwrap().is_none());
    assert!(cache.store_package(&old, &rows, &[], &cancel).is_err());
    assert_eq!(
        cache
            .connection
            .query_row("SELECT count(*) FROM part", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn cancellation_and_failed_writes_leave_previous_cache_intact() {
    let root = tempfile::tempdir().unwrap();
    let (package, rows) = source(&root.path().join("source.package"), 3);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&package, &rows, &[], &cancel).unwrap();
    let mut changed = rows.clone();
    changed[0].casp.name = "replacement snapshot".into();
    let result = cache.store_package_with_progress(&package, &changed, &[], &cancel, |done| {
        if done == 1 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        cache.load_package(&package, &cancel).unwrap().unwrap().rows[0]
            .casp
            .name,
        rows[0].casp.name
    );
    cache.connection.execute_batch("CREATE TEMP TRIGGER injected_failure BEFORE INSERT ON part BEGIN SELECT RAISE(ABORT,'injected cache write failure'); END;").unwrap();
    assert!(
        cache
            .store_package(&package, &changed, &[], &cancel)
            .is_err()
    );
    assert_eq!(
        cache.load_package(&package, &cancel).unwrap().unwrap().rows[0]
            .casp
            .name,
        rows[0].casp.name
    );
    assert!(
        cache
            .store_package(&package, &rows[..1], &[], &cancel)
            .is_err()
    );
}
#[test]
fn another_connection_never_sees_partial_package_publication() {
    let root = tempfile::tempdir().unwrap();
    let (package, rows) = source(&root.path().join("source.package"), 3);
    let db = root.path().join("metadata.sqlite");
    let mut writer = CatalogCache::open(&db).unwrap();
    let cancel = AtomicBool::new(false);
    writer.store_package(&package, &rows, &[], &cancel).unwrap();
    let reader = CatalogCache::open(&db).unwrap();
    let mut changed = rows.clone();
    for row in &mut changed {
        row.casp.name = "new complete snapshot".into();
    }
    let mut observed = false;
    writer
        .store_package_with_progress(&package, &changed, &[], &cancel, |_| {
            let old = reader.load_package(&package, &cancel).unwrap().unwrap();
            assert_eq!(old.rows.len(), 3);
            assert_eq!(old.rows[0].casp.name, rows[0].casp.name);
            observed = true;
        })
        .unwrap();
    assert!(observed);
    let loaded = reader.load_package(&package, &cancel).unwrap().unwrap();
    assert!(
        loaded
            .rows
            .iter()
            .all(|r| r.casp.name == "new complete snapshot")
    );
}
#[test]
fn sqlite_long_work_is_interruptible_and_foreign_database_is_preserved() {
    let root = tempfile::tempdir().unwrap();
    let cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            cancel.store(true, Ordering::Relaxed);
        });
        let _guard = ProgressGuard::new(&cache.connection, &cancel);
        let result=sql(cache.connection.query_row("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n",[],|r|r.get::<_,i64>(0)),&cancel);
        assert!(matches!(result, Err(Error::Cancelled)));
    });
    let other = root.path().join("unrelated.sqlite");
    let db = rusqlite::Connection::open(&other).unwrap();
    db.execute_batch(
        "CREATE TABLE user_data(value TEXT); INSERT INTO user_data VALUES('preserve me');",
    )
    .unwrap();
    drop(db);
    assert!(CatalogCache::open(&other).is_err());
    let db = rusqlite::Connection::open(&other).unwrap();
    assert_eq!(
        db.query_row("SELECT value FROM user_data", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "preserve me"
    );
}
#[test]
#[cfg(windows)]
fn source_write_handle_prevents_cache_publication() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.package");
    let (package, rows) = source(&path, 2);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&package, &rows, &[], &cancel).unwrap();
    let writer = OpenOptions::new().write(true).open(&path).unwrap();
    assert!(cache.store_package(&package, &rows, &[], &cancel).is_err());
    drop(writer);
    assert_eq!(
        cache
            .load_package(&package, &cancel)
            .unwrap()
            .unwrap()
            .rows
            .len(),
        2
    );
}

#[test]
fn search_scope_preserves_duplicate_keys_and_excludes_other_sources() {
    let root = tempfile::tempdir().unwrap();
    let (first, a) = source(&root.path().join("first.package"), 3);
    let (second, b) = source(&root.path().join("second.package"), 3);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&first, &a, &[], &cancel).unwrap();
    cache.store_package(&second, &b, &[], &cancel).unwrap();
    cache
        .bind_scope("one", &[first.fingerprint()], &cancel)
        .unwrap();
    cache
        .bind_scope(
            "both",
            &[first.fingerprint(), second.fingerprint()],
            &cancel,
        )
        .unwrap();
    let one = cache
        .search_scoped("one", "", None, 0, 100, &cancel)
        .unwrap();
    let both = cache
        .search_scoped("both", "", None, 0, 100, &cancel)
        .unwrap();
    assert_eq!(one.len(), 3);
    assert_eq!(both.len(), 6);
    assert!(one.iter().all(|h| h.package_path == first.path));
    assert_eq!(
        both.iter()
            .filter(|h| h.key == first.entries[0].key)
            .count(),
        2
    );
    assert!(matches!(
        cache.search_scoped("unknown", "", None, 0, 10, &cancel),
        Err(Error::UnqualifiedScope(_))
    ));
    cache.bind_scope("empty", &[], &cancel).unwrap();
    assert!(
        cache
            .search_scoped("empty", "", None, 0, 10, &cancel)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn failed_and_cancelled_scope_rebind_preserve_previous_membership() {
    let root = tempfile::tempdir().unwrap();
    let (first, a) = source(&root.path().join("first.package"), 2);
    let (second, b) = source(&root.path().join("second.package"), 2);
    let (missing, _) = source(&root.path().join("missing.package"), 2);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&first, &a, &[], &cancel).unwrap();
    cache.store_package(&second, &b, &[], &cancel).unwrap();
    cache
        .bind_scope("active", &[first.fingerprint()], &cancel)
        .unwrap();
    assert!(
        cache
            .bind_scope(
                "active",
                &[first.fingerprint(), missing.fingerprint()],
                &cancel
            )
            .is_err()
    );
    assert_eq!(
        cache
            .search_scoped("active", "", None, 0, 100, &cancel)
            .unwrap()
            .len(),
        2
    );
    let result = cache.bind_scope_with_progress(
        "active",
        &[first.fingerprint(), second.fingerprint()],
        &cancel,
        |done| {
            if done == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        cache
            .search_scoped("active", "", None, 0, 100, &cancel)
            .unwrap()
            .len(),
        2
    );
    assert!(
        cache
            .bind_scope(
                "active",
                &[first.fingerprint(), first.fingerprint()],
                &cancel
            )
            .is_err()
    );
    assert_eq!(
        cache
            .search_scoped("active", "", None, 0, 100, &cancel)
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn scope_query_rejects_stale_missing_or_partial_cached_packages() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.package");
    let (old, rows) = source(&path, 3);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    cache.store_package(&old, &rows, &[], &cancel).unwrap();
    cache
        .bind_scope("old", &[old.fingerprint()], &cancel)
        .unwrap();
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"new source extent").unwrap();
    drop(file);
    let current = PackageIndex::open(&path).unwrap();
    cache.store_package(&current, &rows, &[], &cancel).unwrap();
    assert!(matches!(
        cache.search_scoped("old", "", None, 0, 100, &cancel),
        Err(Error::UnqualifiedScope(_))
    ));
    cache
        .bind_scope("new", &[current.fingerprint()], &cancel)
        .unwrap();
    cache
        .connection
        .execute("DELETE FROM part WHERE entry_index=0", [])
        .unwrap();
    assert!(matches!(
        cache.search_scoped("new", "", None, 0, 100, &cancel),
        Err(Error::UnqualifiedScope(_))
    ));
    assert!(
        cache
            .bind_scope("partial", &[current.fingerprint()], &cancel)
            .is_err()
    );
    cache.store_package(&current, &rows, &[], &cancel).unwrap();
    cache
        .bind_scope("restored", &[current.fingerprint()], &cancel)
        .unwrap();
    cache.invalidate_package(&current.path, &cancel).unwrap();
    assert!(matches!(
        cache.search_scoped("restored", "", None, 0, 100, &cancel),
        Err(Error::UnqualifiedScope(_))
    ));
}
#[test]
fn v1_upgrade_preserves_metadata_and_qualifies_scope() {
    let root = tempfile::tempdir().unwrap();
    let (package, rows) = source(&root.path().join("source.package"), 3);
    let path = root.path().join("metadata.sqlite");
    let cancel = AtomicBool::new(false);
    let mut cache = CatalogCache::open(&path).unwrap();
    cache.store_package(&package, &rows, &[], &cancel).unwrap();
    cache.connection.execute_batch("DROP TRIGGER package_insert_revision; DROP TRIGGER package_update_revision; DROP TRIGGER package_delete_revision; DROP TRIGGER part_insert_revision; DROP TRIGGER part_update_revision; DROP TRIGGER part_delete_revision; DROP TRIGGER unsupported_insert_revision; DROP TRIGGER unsupported_update_revision; DROP TRIGGER unsupported_delete_revision; DROP TABLE scope_package; DROP TABLE scope; DROP TABLE cache_metadata; ALTER TABLE package DROP COLUMN fingerprint_sha; ALTER TABLE package DROP COLUMN expected_casp_count; PRAGMA user_version=1;").unwrap();
    drop(cache);
    let mut cache = CatalogCache::open(&path).unwrap();
    let loaded = cache.load_package(&package, &cancel).unwrap().unwrap();
    assert_eq!(loaded.rows.len(), rows.len());
    assert_eq!(
        serde_json::to_value(&loaded.rows[0].casp).unwrap(),
        serde_json::to_value(&rows[0].casp).unwrap()
    );
    cache
        .bind_scope("upgraded", &[package.fingerprint()], &cancel)
        .unwrap();
    assert_eq!(
        cache
            .search_scoped("upgraded", "", None, 0, 100, &cancel)
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn scope_membership_has_no_sql_parameter_list_cap() {
    // A correctness fixture above the historical999-variable threshold, not a
    // filesystem scan or performance result. Every insert uses constant arity.
    let root = tempfile::tempdir().unwrap();
    let (package, _) = source(&root.path().join("empty.package"), 0);
    let mut cache = CatalogCache::open(root.path().join("metadata.sqlite")).unwrap();
    let cancel = AtomicBool::new(false);
    let mut expected = Vec::new();
    {
        let tx = cache.connection.transaction().unwrap();
        let mut insert=tx.prepare("INSERT INTO package(path_key,path_label,source_label,file_size,index_sha,modified_ns,created_ns,fingerprint_sha,expected_casp_count) VALUES(?1,?2,'fixture',?3,?4,?5,?6,?7,0)").unwrap();
        for i in 0..1100 {
            let mut fp = package.fingerprint();
            fp.path = root
                .path()
                .join(format!("declared-scope-fixture-{i}.package"));
            let signature = super::scope::fingerprint_signature(&fp);
            insert
                .execute(params![
                    path_key(&fp.path),
                    fp.path.to_string_lossy(),
                    fp.file_size as i64,
                    fp.index_sha256.as_slice(),
                    time_key(fp.modified),
                    time_key(fp.created),
                    signature.as_slice()
                ])
                .unwrap();
            expected.push(fp);
        }
        drop(insert);
        tx.commit().unwrap();
    }
    cache.bind_scope("many", &expected, &cancel).unwrap();
    assert!(
        cache
            .search_scoped("many", "", None, 0, 100, &cancel)
            .unwrap()
            .is_empty()
    );
    cache.release_scope("many", &cancel).unwrap();
    assert!(
        cache
            .search_scoped("many", "", None, 0, 100, &cancel)
            .is_err()
    );
}
