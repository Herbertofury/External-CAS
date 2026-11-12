// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual disposable SQLite durability/conflict tests, no game/profile data.
use external_cas_link::*;
use rusqlite::Connection;
use std::{collections::BTreeMap, sync::Arc};
fn controller() -> LinkController {
    let mut c = LinkController::new(LinkLimits::default());
    c.mode = LinkMode::LiveLink;
    for sim in 1..=3 {
        let id = SimIdentity {
            save: SaveIdentity {
                slot_id: 4,
                world_id: 8,
                lineage: [3; 32],
            },
            household_id: 12,
            sim_id: sim,
            form: Some(1),
        };
        let packed = [
            0xffffffffffffffffu64,
            0x8000000000000001u64,
            0x0123456789abcdefu64,
        ]
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .collect::<Vec<_>>();
        let fields = BTreeMap::from([
            ("outfits-native-protobuf".into(), Arc::from(packed)),
            ("unknown-field".into(), Arc::from(vec![sim as u8, 0, 255])),
        ]);
        let base = Arc::new(
            Snapshot::new(
                id.clone(),
                fields.clone(),
                Coverage::Partial {
                    missing: vec!["fixture-full-sim".into()],
                },
            )
            .unwrap(),
        );
        c.sims.insert(
            id.clone(),
            SimContext {
                base,
                draft: DraftState {
                    fields,
                    ..Default::default()
                },
                remote: None,
                conflict: None,
                departed: false,
                last_operation: None,
            },
        );
        c.edit(
            &ContextId::Sim(id.clone()),
            0,
            "slider-external-draft".into(),
            Some(vec![5, sim as u8]),
        )
        .unwrap();
        c.selected = Some(id);
    }
    c.edit(
        &ContextId::Standalone,
        0,
        "separate-standalone".into(),
        Some(vec![1, 2, 3]),
    )
    .unwrap();
    c
}
#[test]
fn real_sqlite_roundtrip_retains_all_sim_drafts_undo_unknowns_and_full_u64_color_shift_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link.sqlite");
    let mut store = LinkStore::open(&path, StoreLimits::default()).unwrap();
    assert!(store.load(LinkLimits::default()).unwrap().is_none());
    let c = controller();
    let receipt = store.checkpoint(&c, 0).unwrap();
    assert_eq!(receipt.generation, 1);
    drop(store);
    let mut store = LinkStore::open(&path, StoreLimits::default()).unwrap();
    assert!(store.checkpoint(&c, 1).is_err());
    let loaded = store.load(LinkLimits::default()).unwrap().unwrap();
    assert_eq!(loaded.receipt.sha256, receipt.sha256);
    assert_eq!(loaded.controller.sims.len(), 3);
    assert_eq!(loaded.controller.standalone.fields, c.standalone.fields);
    for (id, original) in &c.sims {
        let recovered = &loaded.controller.sims[id];
        assert_eq!(recovered.draft.fields, original.draft.fields);
        assert_eq!(recovered.draft.undo.len(), 1);
        assert_eq!(recovered.base.fields, original.base.fields);
        assert!(recovered.conflict.is_some());
    }
    store.checkpoint(&loaded.controller, 1).unwrap();
}
#[test]
fn cancelled_checkpoint_preserves_prior_generation_and_qualifies_only_after_complete_reload() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link.sqlite");
    let mut store = LinkStore::open(&path, StoreLimits::default()).unwrap();
    let c = controller();
    let prior = store.checkpoint(&c, 0).unwrap();
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert!(matches!(
        store.checkpoint_cancellable(&c, 1, &cancelled),
        Err(LinkError::Cancelled)
    ));
    assert!(matches!(
        store.load_cancellable(LinkLimits::default(), &cancelled),
        Err(LinkError::Cancelled)
    ));
    let actual = store.load(LinkLimits::default()).unwrap().unwrap();
    assert_eq!(actual.receipt.sha256, prior.sha256);
    assert_eq!(actual.receipt.generation, 1);
}
#[test]
fn stale_concurrent_generation_and_failed_sqlite_transaction_leave_previous_complete_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link.sqlite");
    let c = controller();
    let mut first = LinkStore::open(&path, StoreLimits::default()).unwrap();
    first.checkpoint(&c, 0).unwrap();
    let mut second = LinkStore::open(&path, StoreLimits::default()).unwrap();
    second.load(LinkLimits::default()).unwrap();
    let receipt = first.checkpoint(&c, 1).unwrap();
    assert!(matches!(second.checkpoint(&c, 1), Err(LinkError::Stale(_))));
    assert_eq!(
        second
            .load(LinkLimits::default())
            .unwrap()
            .unwrap()
            .receipt
            .sha256,
        receipt.sha256
    );
    let fault = Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER fail_after_first BEFORE INSERT ON sim_context WHEN (SELECT count(*) FROM sim_context)>0 BEGIN SELECT RAISE(ABORT,'intentional disposable write failure');END;").unwrap();
    assert!(first.checkpoint(&c, 2).is_err());
    let loaded = second.load(LinkLimits::default()).unwrap().unwrap();
    assert_eq!(loaded.receipt.generation, 2);
    assert_eq!(loaded.controller.sims.len(), 3);
    assert_eq!(loaded.receipt.sha256, receipt.sha256);
}
#[test]
fn unknown_schema_foreign_database_corrupt_hash_and_budget_never_authorize_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link.sqlite");
    let c = controller();
    let mut store = LinkStore::open(&path, StoreLimits::default()).unwrap();
    store.checkpoint(&c, 0).unwrap();
    drop(store);
    let mut bounded = LinkStore::open(
        &path,
        StoreLimits {
            max_row_bytes: 8,
            max_total_bytes: 16,
        },
    )
    .unwrap();
    assert!(matches!(
        bounded.load(LinkLimits::default()),
        Err(LinkError::Budget(_))
    ));
    assert!(bounded.checkpoint(&c, 1).is_err());
    drop(bounded);
    let raw = Connection::open(&path).unwrap();
    raw.execute("UPDATE checkpoint SET sha256=zeroblob(32)", [])
        .unwrap();
    let mut corrupt = LinkStore::open(&path, StoreLimits::default()).unwrap();
    assert!(corrupt.load(LinkLimits::default()).is_err());
    assert!(corrupt.checkpoint(&c, 1).is_err());
    drop(corrupt);
    raw.pragma_update(None, "user_version", 99).unwrap();
    drop(raw);
    assert!(matches!(
        LinkStore::open(&path, StoreLimits::default()),
        Err(LinkError::Unsupported(_))
    ));
    let raw = Connection::open(&path).unwrap();
    let version: i32 = raw
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 99);
    let foreign = temp.path().join("foreign.sqlite");
    let other = Connection::open(&foreign).unwrap();
    other.execute_batch("CREATE TABLE private_original(value TEXT);INSERT INTO private_original VALUES('retained');").unwrap();
    assert!(LinkStore::open(&foreign, StoreLimits::default()).is_err());
    assert_eq!(
        other
            .query_row("SELECT value FROM private_original", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "retained"
    );
}
#[test]
fn abrupt_writer_exit_before_commit_recovers_last_complete_sqlite_generation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link.sqlite");
    let mut store = LinkStore::open(&path, StoreLimits::default()).unwrap();
    let c = controller();
    let previous = store.checkpoint(&c, 0).unwrap();
    drop(store);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--ignored"])
        .env("CAS_LINK_CRASH_TEST_PATH", &path)
        .output()
        .unwrap();
    assert_eq!(
        child.status.code(),
        Some(87),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let store = LinkStore::open(&path, StoreLimits::default()).unwrap();
    let recovered = store.load(LinkLimits::default()).unwrap().unwrap();
    assert_eq!(recovered.receipt.generation, 1);
    assert_eq!(recovered.receipt.sha256, previous.sha256);
    assert_eq!(recovered.controller.sims.len(), 3);
}
#[test]
#[ignore = "launched only as a disposable abrupt-exit child"]
fn crash_child() {
    let path = std::env::var_os("CAS_LINK_CRASH_TEST_PATH").expect("disposable parent path");
    let connection = Connection::open(path).unwrap();
    connection.execute_batch("PRAGMA cache_size=4;BEGIN IMMEDIATE;DELETE FROM sim_context;UPDATE checkpoint SET generation=999;INSERT INTO operation VALUES(zeroblob(16),zeroblob(1048576));").unwrap();
    std::process::exit(87);
}
