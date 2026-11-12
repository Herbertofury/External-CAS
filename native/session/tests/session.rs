use external_cas_session::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "external-cas-session-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }
    fn store(&self) -> SessionStore {
        SessionStore::at(&self.0).unwrap()
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        // Only flat files from this exclusive test root are removed. No recursive
        // deletion or following links, including when a test failed midway.
        if let Ok(entries) = fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_file()) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        let _ = fs::remove_dir(&self.0);
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(root: &PrivateRoot, name: &str, schema: u32, payload: Value) -> PathBuf {
    let payload = serde_json::to_string(&payload).unwrap();
    let snapshot = format!(
        "{{\"format\":\"external-cas-session\",\"schema\":{schema},\"created_unix_ns\":\"1\",\"payload_sha256\":\"{}\",\"payload\":{payload}}}",
        hash(payload.as_bytes())
    );
    let path = root.0.join(name);
    fs::write(&path, snapshot.as_bytes()).unwrap();
    commit(&path, 1);
    path
}
fn commit(path: &std::path::Path, schema: u32) {
    let bytes = fs::read(path).unwrap();
    fs::write(path.with_extension("complete"),serde_json::to_vec(&json!({"format":"external-cas-session-commit","schema":schema,"file_name":path.file_name().unwrap().to_str().unwrap(),"snapshot_sha256":hash(&bytes),"byte_length":bytes.len()})).unwrap()).unwrap();
}
fn latest_name() -> &'static str {
    "session-999999999999999999999999999999999999999-0000000000-00000000000000000000.json"
}

#[test]
fn complete_context_roundtrips_and_preserves_unknown_same_schema_fields() {
    let root = PrivateRoot::new();
    let store = root.store();
    let mut state = SessionState::default();
    state.roots = vec![root.0.join("game assets"), root.0.join("creator assets")];
    state.project_path = Some(root.0.join("verified-project"));
    state.recovery_project_path = Some(root.0.join("unsaved-draft"));
    let mut selection = ResourceSelection::new(
        "034AEECB:00000000:0123456789ABCDEF",
        root.0.join("selected.package"),
    );
    selection.payload_sha256 = Some("a".repeat(64));
    selection.source_index_sha256 = Some("b".repeat(64));
    selection
        .extra
        .insert("future-selection-mode".into(), json!({"value":8}));
    state.selection = Some(selection);
    state.rig = Some(PinnedFile::new(
        root.0.join("explicit.grannyrig"),
        "c".repeat(64),
    ));
    let mut clip = ClipSelection::new(
        "6B20C4F3:00000000:1234567890ABCDEF",
        root.0.join("poses.package"),
        "d".repeat(64),
    );
    clip.source_index_sha256 = Some("e".repeat(64));
    state.clip = Some(clip);
    state.tick = 15.25;
    state.camera = CameraState {
        yaw: 0.5,
        pitch: 0.2,
        distance: 4.,
        target: [0.2, 0.9, -0.1],
        ..Default::default()
    };
    state.camera.extra.insert("future-lens".into(), json!(42));
    state.ui_tab = "poses".into();
    state
        .extra
        .insert("future-state".into(), json!([1,2,{"preserved":true}]));
    let first = store.save(&state).unwrap();
    assert!(first.path.exists());
    assert!(first.marker_path.exists());
    let report = store.load().unwrap();
    assert_eq!(report.state, Some(state.clone()));
    assert!(!report.recovered_from_previous);
    assert!(report.issues.is_empty());
    let second = store.save(&report.state.unwrap()).unwrap();
    assert_ne!(first.path, second.path);
    assert!(first.path.exists());
    assert_eq!(store.load().unwrap().state, Some(state));
}
#[test]
fn torn_newest_snapshot_or_marker_falls_back_without_modifying_previous() {
    let root = PrivateRoot::new();
    let store = root.store();
    let first = store.save(&SessionState::default()).unwrap();
    let original = fs::read(&first.path).unwrap();
    let second = store
        .save(&SessionState {
            ui_tab: "project".into(),
            ..Default::default()
        })
        .unwrap();
    let second_bytes = fs::read(&second.path).unwrap();
    for n in [0, 1, second_bytes.len() / 2, second_bytes.len() - 1] {
        fs::write(&second.path, &second_bytes[..n]).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.state.unwrap().ui_tab, "wardrobe");
        assert!(loaded.recovered_from_previous);
        assert!(loaded.issues.iter().any(|i| i.kind == IssueKind::Invalid));
        assert_eq!(fs::read(&first.path).unwrap(), original);
    }
    fs::write(&second.path, &second_bytes).unwrap();
    let marker = fs::read(&second.marker_path).unwrap();
    fs::write(&second.marker_path, &marker[..marker.len() / 2]).unwrap();
    assert_eq!(store.load().unwrap().state.unwrap().ui_tab, "wardrobe");
}
#[test]
fn uncommitted_snapshot_is_retained_and_never_loaded() {
    let root = PrivateRoot::new();
    let store = root.store();
    store.save(&SessionState::default()).unwrap();
    let incomplete = root.0.join(latest_name());
    fs::write(&incomplete, b"{\"private\":\"torn").unwrap();
    let report = store.load().unwrap();
    assert_eq!(report.state.unwrap().ui_tab, "wardrobe");
    assert!(report.recovered_from_previous);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::Incomplete)
    );
    assert!(incomplete.exists());
    store
        .save(&SessionState {
            ui_tab: "sims".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fs::read(&incomplete).unwrap(), b"{\"private\":\"torn");
}
#[test]
fn unknown_future_schema_is_preserved_falls_back_and_prevents_downgrade_writes() {
    let root = PrivateRoot::new();
    let store = root.store();
    store.save(&SessionState::default()).unwrap();
    let future = fixture(
        &root,
        latest_name(),
        2,
        json!({"futurePrivateState":"do not reinterpret"}),
    );
    let original = fs::read(&future).unwrap();
    let report = store.load().unwrap();
    assert_eq!(report.state.unwrap().ui_tab, "wardrobe");
    assert_eq!(report.future_schemas, [2]);
    assert!(report.recovered_from_previous);
    assert!(matches!(
        store.save(&SessionState::default()),
        Err(SessionError::FutureSchema { .. })
    ));
    assert_eq!(fs::read(&future).unwrap(), original);
}
#[test]
fn future_commit_marker_is_retained_without_assuming_its_payload_schema() {
    let root = PrivateRoot::new();
    let store = root.store();
    store.save(&SessionState::default()).unwrap();
    let future = fixture(&root, latest_name(), 99, json!({"encoding":"future"}));
    commit(&future, 2);
    let report = store.load().unwrap();
    assert_eq!(report.future_schemas, [2]);
    assert!(matches!(
        store.save(&SessionState::default()),
        Err(SessionError::FutureSchema { .. })
    ));
    assert!(future.exists());
}
#[test]
fn payload_hash_mismatch_refuses_a_recommitted_but_changed_known_snapshot() {
    let root = PrivateRoot::new();
    let store = root.store();
    store.save(&SessionState::default()).unwrap();
    let corrupt = fixture(&root, latest_name(), 1, json!({"ui_tab":"project"}));
    let bytes = fs::read_to_string(&corrupt)
        .unwrap()
        .replace("project", "changed");
    fs::write(&corrupt, bytes).unwrap();
    commit(&corrupt, 1);
    let report = store.load().unwrap();
    assert_eq!(report.state.unwrap().ui_tab, "wardrobe");
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.message.contains("payload hash"))
    );
}
#[test]
fn concurrent_writers_commit_distinct_snapshots_without_overwriting() {
    let root = PrivateRoot::new();
    let store = Arc::new(root.store());
    let writers = (0..24)
        .map(|i| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .save(&SessionState {
                        ui_tab: format!("writer-{i}"),
                        ..Default::default()
                    })
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let mut paths = writers
        .into_iter()
        .map(|w| w.join().unwrap().path)
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    assert_eq!(paths.len(), 24);
    for path in &paths {
        assert!(path.exists());
        assert!(path.with_extension("complete").exists());
    }
    let report = store.load().unwrap();
    assert!(report.state.unwrap().ui_tab.starts_with("writer-"));
    assert!(report.issues.is_empty());
}
#[test]
fn failed_writes_leave_existing_context_and_user_files_untouched() {
    let root = PrivateRoot::new();
    let store = root.store();
    let receipt = store.save(&SessionState::default()).unwrap();
    let before = fs::read(&receipt.path).unwrap();
    let blocking = root.0.join("not-a-directory");
    fs::write(&blocking, b"original").unwrap();
    let bad = SessionStore::at(&blocking).unwrap();
    assert!(bad.save(&SessionState::default()).is_err());
    assert_eq!(fs::read(&blocking).unwrap(), b"original");
    assert_eq!(fs::read(&receipt.path).unwrap(), before);
}
#[test]
fn source_inventory_has_no_count_cap_and_oversize_payload_fails_without_truncation() {
    let root = PrivateRoot::new();
    let store = root.store();
    let mut state = SessionState::default();
    state.roots = (0..10001)
        .map(|i| root.0.join(format!("authorized-source-{i}")))
        .collect();
    store.save(&state).unwrap();
    assert_eq!(store.load().unwrap().state.unwrap().roots.len(), 10001);
    state
        .extra
        .insert("oversize".into(), json!("x".repeat(MAX_PAYLOAD_BYTES)));
    let before = fs::read_dir(&root.0).unwrap().count();
    assert!(store.save(&state).is_err());
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), before);
    assert_eq!(store.load().unwrap().state.unwrap().roots.len(), 10001);
}
#[test]
fn invalid_context_is_not_persisted_and_empty_load_creates_nothing() {
    let root = PrivateRoot::new();
    let absent = root.0.join("absent");
    let store = SessionStore::at(&absent).unwrap();
    assert!(store.load().unwrap().state.is_none());
    assert!(!absent.exists());
    let mut state = SessionState::default();
    state.tick = f32::NAN;
    assert!(store.save(&state).is_err());
    state.tick = 0.;
    state.selection = Some(ResourceSelection::new("1234", root.0.join("x.package")));
    assert!(store.save(&state).is_err());
    state.selection = None;
    state.roots.push(PathBuf::from("relative"));
    assert!(store.save(&state).is_err());
    assert!(!absent.exists());
}
