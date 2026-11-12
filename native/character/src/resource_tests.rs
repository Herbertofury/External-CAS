use super::*;
use external_cas_scene::GameInstallScope;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn fixture() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target-fixtures")
        .join(format!(
            "providers-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(root.join("Data/Client")).unwrap();
    fs::create_dir_all(root.join("Data/Simulation")).unwrap();
    fs::write(root.join("Data/Client/Resource.cfg"), b"").unwrap();
    fs::write(root.join("Data/Simulation/Resource.cfg"), b"").unwrap();
    fs::write(
        root.join("Data/Client/Resource.cfg"),
        "Priority -30\nPackedFile Old.package\nPriority -20\nPackedFile Patch*.package\n",
    )
    .unwrap();
    root
}
fn package(root: &Path, name: &str, key: ResourceKey, payload: &[u8], compression: u16) {
    let mut b = vec![0u8; 96];
    b[..4].copy_from_slice(b"DBPF");
    b[4..8].copy_from_slice(&2u32.to_le_bytes());
    b[8..12].copy_from_slice(&1u32.to_le_bytes());
    b[36..40].copy_from_slice(&1u32.to_le_bytes());
    b.extend(payload);
    let at = b.len() as u32;
    let mut index = 0u32.to_le_bytes().to_vec();
    for n in [
        key.type_id,
        key.group_id,
        (key.instance_id >> 32) as u32,
        key.instance_id as u32,
        96,
        0x80000000 | payload.len() as u32,
        payload.len() as u32,
    ] {
        index.extend(n.to_le_bytes());
    }
    index.extend(compression.to_le_bytes());
    index.extend(1u16.to_le_bytes());
    b[44..48].copy_from_slice(&(index.len() as u32).to_le_bytes());
    b[64..68].copy_from_slice(&at.to_le_bytes());
    b.extend(index);
    fs::write(root.join("Data/Client").join(name), b).unwrap();
}
#[test]
fn only_effective_priority_ties_decide_payload_equivalence_but_all_hashes_remain() {
    let key = ResourceKey::new(0x067caa11, 0, 42);
    for different in [false, true] {
        let root = fixture();
        package(&root, "Old.package", key, b"old", 0);
        package(&root, "PatchA.package", key, b"new", 0);
        package(
            &root,
            "PatchB.package",
            key,
            if different { b"bad" } else { b"new" },
            0,
        );
        let cancel = AtomicBool::new(false);
        let catalog = Catalog::scan_roots(&[root.clone()], &cancel, |_| {}).unwrap();
        let scope = GameInstallScope::discover(&root).unwrap();
        let policy = scope.prepare(&catalog).unwrap();
        assert_eq!(
            policy.resolve_key(key).state,
            EffectiveState::TiedHighestPriority
        );
        let choices = BTreeMap::new();
        let mut resolver = Resolver::new(
            &catalog,
            Some(&policy),
            &choices,
            ReadLimits::default(),
            1024,
            &cancel,
        );
        let result = resolver.get(key);
        assert_eq!(
            result
                .candidates
                .iter()
                .filter(|c| c.payload_sha256.is_some())
                .count(),
            3
        );
        if different {
            assert!(result.bytes.is_none());
            assert!(result.error.is_some());
        } else {
            assert_eq!(result.bytes.as_deref(), Some(b"new".as_slice()));
            assert!(result.selection.contains("highest-priority"));
        }
        let mut unscoped = Resolver::new(
            &catalog,
            None,
            &choices,
            ReadLimits::default(),
            1024,
            &cancel,
        );
        assert!(unscoped.get(key).bytes.is_none());
    }
}
#[test]
fn highest_priority_tombstone_does_not_restore_old_payload() {
    let key = ResourceKey::new(0x067caa11, 0, 42);
    let root = fixture();
    package(&root, "Old.package", key, b"old", 0);
    package(&root, "PatchA.package", key, b"", 0xffe0);
    let cancel = AtomicBool::new(false);
    let catalog = Catalog::scan_roots(&[root.clone()], &cancel, |_| {}).unwrap();
    let scope = GameInstallScope::discover(&root).unwrap();
    let policy = scope.prepare(&catalog).unwrap();
    assert_eq!(
        policy.resolve_key(key).state,
        EffectiveState::DeletedHighestPriority
    );
    let choices = BTreeMap::new();
    let mut resolver = Resolver::new(
        &catalog,
        Some(&policy),
        &choices,
        ReadLimits::default(),
        1024,
        &cancel,
    );
    let result = resolver.get(key);
    assert!(result.bytes.is_none());
    assert!(result.error.as_ref().unwrap().contains("deletion"));
}
