use external_cas_tray::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const META: &str = "0x00000000!0x000000000000004d.trayitem";
const HOUSE: &str = "0x00000000!0x000000000000004d.householdbinary";
const PORTRAIT: &str = "0x00000013!0x0000000000000384.sgi";

// Fixture encoders are deliberately independent of the crate's private reader.
fn vint(mut v: u64) -> Vec<u8> {
    let mut b = vec![];
    loop {
        let mut c = (v & 127) as u8;
        v >>= 7;
        if v != 0 {
            c |= 128;
        }
        b.push(c);
        if v == 0 {
            return b;
        }
    }
}
fn integer(n: u32, v: u64) -> Vec<u8> {
    let mut b = vint((n as u64) << 3);
    b.extend(vint(v));
    b
}
fn fixed(n: u32, v: u64) -> Vec<u8> {
    let mut b = vint(((n as u64) << 3) | 1);
    b.extend(v.to_le_bytes());
    b
}
fn float(n: u32, v: f32) -> Vec<u8> {
    let mut b = vint(((n as u64) << 3) | 5);
    b.extend(v.to_le_bytes());
    b
}
fn blob(n: u32, v: &[u8]) -> Vec<u8> {
    let mut b = vint(((n as u64) << 3) | 2);
    b.extend(vint(v.len() as u64));
    b.extend(v);
    b
}
fn join(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}
fn sim() -> Vec<u8> {
    join(&[
        fixed(1, 123),
        fixed(4, 999),
        blob(5, b"Mara"),
        blob(6, b"Moon"),
        integer(7, 2),
        integer(8, 32),
        integer(10, 0xabc),
        blob(500, b"unknown appearance and genetics"),
        integer(501, 987),
        blob(
            21,
            &join(&[blob(1, &outfit(true)), blob(1, &outfit(false))]),
        ),
    ])
}
fn outfit(packed: bool) -> Vec<u8> {
    let ids = [0x1122334455667788u64, 0x8877665544332211u64];
    let parts = if packed {
        blob(
            1,
            &ids.iter().flat_map(|n| n.to_le_bytes()).collect::<Vec<_>>(),
        )
    } else {
        join(&[fixed(1, ids[0]), fixed(1, ids[1])])
    };
    join(&[
        integer(1, if packed { 12345 } else { 12346 }),
        integer(2, 0),
        blob(5, &join(&[parts, blob(700, b"unknown id-list")])),
        blob(7, &blob(1, &[8, 4])),
        blob(12, &blob(1, &[0, 0])),
        blob(701, b"unknown outfit parallel arrays"),
    ])
}
fn family() -> Vec<u8> {
    family_with_sim(&sim())
}
fn family_with_sim(sim_raw: &[u8]) -> Vec<u8> {
    let mut account = join(&[
        integer(1, 42),
        integer(2, 888),
        blob(3, b"Different family identity"),
        blob(6, sim_raw),
        blob(502, b"unknown account"),
    ]);
    // Unknown protobuf groups are legal and must survive without interpretation.
    account.extend(vint((503 << 3) | 3));
    account.extend(integer(1, 11));
    account.extend(vint((503 << 3) | 4));
    join(&[
        blob(1, &account),
        integer(3, 123),
        blob(504, b"unknown family"),
    ])
}
fn image_section() -> Vec<u8> {
    let mut b = vec![7];
    b.extend(2u32.to_le_bytes());
    for (id, data) in [
        (40u64, b"original portrait".as_slice()),
        (41u64, b"second image".as_slice()),
    ] {
        b.extend(id.to_le_bytes());
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
    }
    b
}
fn household_with(payload: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut b = 2u32.to_le_bytes().to_vec();
    b.extend((12 + payload.len() as u32 + suffix.len() as u32).to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend((payload.len() as u32).to_le_bytes());
    b.extend(payload);
    b.extend(suffix);
    b
}
fn household() -> Vec<u8> {
    household_with(&family(), &image_section())
}
fn metadata() -> Vec<u8> {
    // Metadata Sim ID 900 is intentionally distinct from native Sim ID 123.
    let sim = join(&[
        blob(3, b"Mara"),
        blob(4, b"Moon"),
        integer(5, 900),
        integer(6, 2),
        integer(9, 32),
        blob(600, b"unknown metadata sim"),
    ]);
    let hh = join(&[
        integer(1, 1),
        blob(2, &sim),
        blob(601, b"unknown household metadata"),
    ]);
    let payload = join(&[
        integer(1, 77),
        blob(4, b"Moon household"),
        blob(10, &blob(2, &hh)),
        blob(602, b"unknown root metadata"),
    ]);
    let mut result = 0u32.to_le_bytes().to_vec();
    result.extend((payload.len() as u32).to_le_bytes());
    result.extend(payload);
    result
}
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target-fixtures");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "tray-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn library(&self) -> PathBuf {
        let root = self.root.join("source-library");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(META), metadata()).unwrap();
        fs::write(root.join(HOUSE), household()).unwrap();
        fs::write(root.join(PORTRAIT), b"opaque encrypted image bytes").unwrap();
        root
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let intended = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target-fixtures")
            .canonicalize()
            .unwrap();
        if let Ok(actual) = self.root.canonicalize() {
            assert!(actual.starts_with(&intended));
            fs::remove_dir_all(actual).unwrap();
        }
    }
}

#[test]
fn real_schema_fields_and_distinct_ids_are_projected_without_reencoding() {
    let bytes = household();
    let doc = TrayDocument::from_bytes(HOUSE, bytes.clone()).unwrap();
    assert_eq!(doc.bytes(), bytes);
    assert_eq!(doc.native_sims().unwrap(), vec![sim().as_slice()]);
    let DocumentSummary::Household(h) = &doc.summary else {
        panic!()
    };
    assert_eq!(h.family_id, Some(888));
    assert_eq!(h.account_id, 42);
    assert_eq!(h.sims[0].sim_id, Some(123));
    assert_eq!(h.sims[0].household_id, Some(999));
    assert_eq!(h.sims[0].first_name, "Mara");
    assert_eq!(h.sims[0].last_name, "Moon");
    assert_eq!(h.sims[0].skin_tone, Some(0xabc));
    let images = h.embedded_images.as_ref().unwrap();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].id, 40);
    assert_eq!(images[0].sha256, sha256(b"original portrait"));
    assert_eq!(&doc.bytes()[h.suffix_range.clone()], image_section());
    let meta = TrayDocument::from_bytes(META, metadata()).unwrap();
    let DocumentSummary::Metadata(m) = meta.summary else {
        panic!()
    };
    assert_eq!(m.tray_id, Some(77));
    assert_eq!(m.sims[0].sim_id, Some(900));
}

#[test]
fn opaque_suffix_is_retained_and_cannot_be_mistaken_for_a_count() {
    let suffix = b"not a member count or known image section";
    let bytes = household_with(&family(), suffix);
    let doc = TrayDocument::from_bytes(HOUSE, bytes.clone()).unwrap();
    let DocumentSummary::Household(h) = &doc.summary else {
        panic!()
    };
    assert!(h.embedded_images.is_none());
    assert_eq!(h.warnings.len(), 1);
    assert_eq!(&doc.bytes()[h.suffix_range.clone()], suffix);
    assert_eq!(doc.bytes(), bytes);
}

#[test]
fn every_truncated_payload_fails_even_with_a_repaired_outer_extent() {
    let payload = family();
    for cut in 0..payload.len() {
        let raw = household_with(&payload[..cut], &image_section());
        // Cutting exactly after a complete field can leave a structurally valid
        // proto2 message; require failure whenever the required account/Sim is cut.
        if cut
            < blob(
                1,
                &join(&[
                    integer(1, 42),
                    integer(2, 888),
                    blob(3, b"Different family identity"),
                    blob(6, &sim()),
                    blob(502, b"unknown account"),
                ]),
            )
            .len()
        {
            assert!(
                TrayDocument::from_bytes(HOUSE, raw).is_err(),
                "accepted payload cut {cut}"
            );
        }
    }
    let raw = household();
    for cut in 0..16 {
        assert!(TrayDocument::from_bytes(HOUSE, raw[..cut].to_vec()).is_err());
    }
}

#[test]
fn wrong_wire_duplicate_owners_and_varint_overflow_fail() {
    for account in [
        join(&[integer(1, 42), integer(1, 43)]),
        join(&[integer(1, 42), blob(6, &integer(1, 123))]),
        join(&[
            integer(1, 42),
            blob(6, &join(&[fixed(1, 123), fixed(1, 124)])),
        ]),
    ] {
        assert!(TrayDocument::from_bytes(
            HOUSE,
            household_with(&blob(1, &account), &[0, 0, 0, 0, 0])
        )
        .is_err());
    }
    let mut overflow = vec![8];
    overflow.extend([255; 10]);
    assert!(TrayDocument::from_bytes(HOUSE, household_with(&overflow, &[])).is_err());
}

#[test]
fn exact_metadata_association_finds_distinct_sim_portrait_and_ignores_prefix_neighbors() {
    let fixture = Fixture::new();
    let root = fixture.library();
    fs::write(
        root.join("0x00000013!0x000000000000007b.sgi"),
        b"native sim ID is not metadata ID",
    )
    .unwrap();
    fs::write(
        root.join("0x00000000!0x000000000000004e.householdbinary"),
        household(),
    )
    .unwrap();
    let library = scan_library(&root).unwrap();
    assert_eq!(library.groups.len(), 1);
    assert!(library.issues.is_empty());
    let group = &library.groups[0];
    assert_eq!(group.paths().len(), 3);
    assert!(group.portrait_paths[0].ends_with(PORTRAIT));
    assert_eq!(group.metadata.sims[0].sim_id, Some(900));
    assert!(group.household_path.as_ref().unwrap().ends_with(HOUSE));
    assert!(scan_library_with_cancel(&root, &AtomicBool::new(true)).is_err());
}

#[test]
fn project_reopen_preserves_every_byte_and_detects_source_and_snapshot_changes() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("first-project")).unwrap();
    assert!(project.check_source_conflicts().is_empty());
    let reopened = reopen_project(&project.directory).unwrap();
    assert_eq!(reopened.documents.len(), 3);
    for doc in &reopened.documents {
        assert_eq!(doc.bytes(), fs::read(root.join(&doc.filename)).unwrap());
    }
    assert_eq!(reopened.household().unwrap().sims[0].sim_id, Some(123));
    fs::write(root.join(PORTRAIT), b"newer portrait").unwrap();
    let conflicts = reopened.check_source_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].path.ends_with(PORTRAIT));
    // Imported project still reopens independently of newer originals.
    assert!(reopen_project(&project.directory).is_ok());
    fs::write(
        project.directory.join("native").join(PORTRAIT),
        b"tampered snapshot",
    )
    .unwrap();
    assert!(matches!(
        reopen_project(&project.directory),
        Err(TrayError::Conflict(_))
    ));
}

#[test]
fn no_overwrite_and_incomplete_recovery_are_enforced() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let destination = fixture.root.join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"important").unwrap();
    assert!(create_project(&group, &destination).is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"important");
    assert!(create_project(&group, &fixture.root.join("Tray")).is_err());
    assert!(create_project(&group, &root.join("nested-project")).is_err());
    let project = create_project(&group, &fixture.root.join("project")).unwrap();
    fs::write(
        project.directory.join("SNAPSHOT_INCOMPLETE"),
        b"interrupted",
    )
    .unwrap();
    assert!(reopen_project(&project.directory).is_err());
    assert_eq!(fs::read(root.join(HOUSE)).unwrap(), household());
}

#[test]
fn stale_scan_and_manifest_path_traversal_are_rejected() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let mut group = scan_library(&root).unwrap().groups.remove(0);
    group.metadata.name = "stale name".into();
    let destination = fixture.root.join("stale");
    assert!(create_project(&group, &destination).is_err());
    assert!(!destination.exists());
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("project")).unwrap();
    let mut manifest = project.manifest.clone();
    manifest.files[0].filename = "../outside.trayitem".into();
    fs::write(
        project.directory.join("ExternalCasProject.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(reopen_project(&project.directory).is_err());
}

#[test]
fn corrupt_metadata_and_ambiguous_household_stay_visible_as_issues() {
    let fixture = Fixture::new();
    let root = fixture.library();
    fs::write(
        root.join("0x00000001!0x000000000000004d.householdbinary"),
        household(),
    )
    .unwrap();
    let library = scan_library(&root).unwrap();
    assert!(library.groups.is_empty());
    assert_eq!(library.issues.len(), 1);
    fs::remove_file(root.join("0x00000001!0x000000000000004d.householdbinary")).unwrap();
    fs::write(root.join(META), b"corrupt").unwrap();
    let library = scan_library(&root).unwrap();
    assert_eq!(library.issues.len(), 1);
}

#[test]
fn invalid_and_non_ascii_names_never_panic() {
    for name in [
        "x",
        "../../anything.trayitem",
        "0x00000000!0x00000000000000zz.hhi",
        "0x000é000!0x000000000000004d.hhi",
    ] {
        assert!(TrayName::parse(name).is_err());
    }
}

#[test]
fn interrupted_copy_is_marked_incomplete_and_originals_remain_untouched() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let destination = fixture.root.join("cancelled");
    let result = create_project_with_progress(&group, &destination, |p| {
        !(p.stage == SnapshotStage::CopyWritten && p.files_completed == 1)
    });
    assert!(matches!(result, Err(TrayError::Cancelled)));
    assert!(destination.join("SNAPSHOT_INCOMPLETE").is_file());
    assert!(!destination.join("ExternalCasProject.json").exists());
    assert!(reopen_project(&destination).is_err());
    assert_eq!(fs::read(root.join(HOUSE)).unwrap(), household());
    assert_eq!(fs::read(root.join(META)).unwrap(), metadata());
}

#[test]
fn actual_io_write_failure_keeps_recovery_copy_and_never_overwrites_collision() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let destination = fixture.root.join("io-failed");
    let result = create_project_with_progress(&group, &destination, |p| {
        if p.stage == SnapshotStage::SnapshotPrepared {
            // Simulate a concurrent filesystem change: the next destination
            // native file is occupied by an existing directory.
            fs::create_dir(destination.join("native").join(HOUSE)).unwrap();
        }
        true
    });
    assert!(matches!(result, Err(TrayError::Io(_))));
    assert!(destination.join("native").join(HOUSE).is_dir());
    assert!(destination.join("SNAPSHOT_INCOMPLETE").is_file());
    assert!(reopen_project(&destination).is_err());
    assert_eq!(fs::read(root.join(HOUSE)).unwrap(), household());
    assert_eq!(
        fs::read(root.join(PORTRAIT)).unwrap(),
        b"opaque encrypted image bytes"
    );
}

#[test]
fn packed_and_unpacked_slot_drafts_preserve_all_other_bytes_and_save_baseline_pins() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("baseline")).unwrap();
    let mut draft = ProjectDraft::from_project(&project).unwrap();
    let slots = draft.slots().unwrap();
    assert_eq!(slots.len(), 4);
    assert_eq!(slots[0].part_id, slots[2].part_id);
    assert_eq!(slots[0].body_type, 8);
    let original = project
        .documents
        .iter()
        .find(|d| d.filename == HOUSE)
        .unwrap()
        .bytes()
        .to_vec();
    let replacement = 0x99aabbccddeeff00;
    draft.apply_slot(0, &slots[0], replacement, 8).unwrap();
    assert_eq!(draft.revision(), 1);
    assert!(draft.can_undo());
    let edited = draft
        .documents()
        .iter()
        .find(|d| d.filename == HOUSE)
        .unwrap()
        .bytes()
        .to_vec();
    let changed = original
        .iter()
        .zip(&edited)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(changed.len(), 8);
    assert_eq!(changed[7] - changed[0], 7);
    assert_eq!(original.len(), edited.len());
    assert_eq!(draft.slots().unwrap()[0].part_id, replacement);
    assert_eq!(draft.slots().unwrap()[2].part_id, slots[2].part_id);
    assert!(draft.apply_slot(0, &slots[0], 11, 8).is_err());
    draft.undo(1).unwrap();
    assert_eq!(
        draft
            .documents()
            .iter()
            .find(|d| d.filename == HOUSE)
            .unwrap()
            .bytes(),
        original
    );
    draft.redo(2).unwrap();
    assert_eq!(
        draft
            .documents()
            .iter()
            .find(|d| d.filename == HOUSE)
            .unwrap()
            .bytes(),
        edited
    );
    let saved = draft.save_as(&fixture.root.join("edited")).unwrap();
    let reopened = reopen_project(&saved.directory).unwrap();
    assert_eq!(reopened.manifest.schema, 2);
    assert!(!reopened.manifest.native_editing_qualified);
    assert!(reopened.check_source_conflicts().is_empty());
    let f = reopened
        .manifest
        .files
        .iter()
        .find(|f| f.filename == HOUSE)
        .unwrap();
    assert_eq!(
        f.original_sha256.as_deref(),
        Some(sha256(&original).as_str())
    );
    assert_eq!(f.sha256, sha256(&edited));
    assert_ne!(f.sha256, f.original_sha256.as_ref().unwrap().as_str());
    for doc in &reopened.documents {
        if doc.filename != HOUSE {
            assert_eq!(
                doc.bytes(),
                project
                    .documents
                    .iter()
                    .find(|d| d.filename == doc.filename)
                    .unwrap()
                    .bytes()
            );
        }
    }
    assert_eq!(
        ProjectDraft::from_project(&reopened)
            .unwrap()
            .slots()
            .unwrap()[0]
            .part_id,
        replacement
    );
    assert_eq!(fs::read(root.join(HOUSE)).unwrap(), original);
    assert!(draft.save_as(&saved.directory).is_err());
    // The earlier project is preserved and remains independently usable.
    assert_eq!(
        reopen_project(&project.directory)
            .unwrap()
            .documents
            .iter()
            .find(|d| d.filename == HOUSE)
            .unwrap()
            .bytes(),
        original
    );
}

#[test]
fn draft_rejects_old_identity_type_and_revision_conflicts_without_mutation() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("baseline")).unwrap();
    let mut draft = ProjectDraft::from_project(&project).unwrap();
    let slot = draft.slots().unwrap().remove(0);
    let hashes = draft
        .documents()
        .iter()
        .map(|d| d.sha256.clone())
        .collect::<Vec<_>>();
    let mut forged = slot.clone();
    forged.part_id += 1;
    assert!(draft.apply_slot(0, &forged, 10, slot.body_type).is_err());
    assert!(draft.apply_slot(0, &slot, 10, slot.body_type + 1).is_err());
    assert!(draft.apply_slot(1, &slot, 10, slot.body_type).is_err());
    assert!(draft.apply_slot(0, &slot, 0, slot.body_type).is_err());
    assert_eq!(draft.revision(), 0);
    assert_eq!(
        hashes,
        draft
            .documents()
            .iter()
            .map(|d| d.sha256.clone())
            .collect::<Vec<_>>()
    );
    draft.apply_slot(0, &slot, 11, slot.body_type).unwrap();
    draft.undo(1).unwrap();
    let refreshed = draft.slots().unwrap().remove(0);
    draft
        .apply_slot(2, &refreshed, 12, refreshed.body_type)
        .unwrap();
    assert!(!draft.can_redo());
}

#[test]
fn changed_original_and_changed_schema_block_draft_commit() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("baseline")).unwrap();
    let mut draft = ProjectDraft::from_project(&project).unwrap();
    let slot = draft.slots().unwrap().remove(0);
    draft.apply_slot(0, &slot, 22, slot.body_type).unwrap();
    fs::write(root.join(PORTRAIT), b"newer user version").unwrap();
    let dest = fixture.root.join("blocked");
    assert!(draft.save_as(&dest).is_err());
    assert!(!dest.exists());
    fs::write(root.join(PORTRAIT), b"opaque encrypted image bytes").unwrap();
    let saved = draft.save_as(&fixture.root.join("edited")).unwrap();
    let mut forged = saved.manifest.clone();
    forged.draft.as_mut().unwrap().schema_evidence_id = "unrecognized schema".into();
    fs::write(
        saved.directory.join("ExternalCasProject.json"),
        serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();
    assert!(reopen_project(&saved.directory).is_err());
}

#[test]
fn malformed_outfit_arrays_do_not_become_editable() {
    let fixture = Fixture::new();
    let root = fixture.library();
    for (body, shifts) in [
        (blob(1, &[8]), blob(1, &[0, 0])),
        (blob(1, &[8, 4]), blob(1, &[0])),
    ] {
        let outfit = join(&[
            integer(1, 45),
            blob(5, &join(&[fixed(1, 100), fixed(1, 200)])),
            blob(7, &body),
            blob(12, &shifts),
        ]);
        let sim = join(&[fixed(1, 123), blob(21, &blob(1, &outfit))]);
        fs::write(
            root.join(HOUSE),
            household_with(&family_with_sim(&sim), &image_section()),
        )
        .unwrap();
        let group = scan_library(&root).unwrap().groups.remove(0);
        let dest = fixture.root.join(format!(
            "bad-{}",
            body.len() + shifts.len() + body[1] as usize
        ));
        // Use distinct owned directory for each malformed fixture.
        let dest = if dest.exists() {
            fixture.root.join("bad-second")
        } else {
            dest
        };
        let project = create_project(&group, &dest).unwrap();
        assert!(ProjectDraft::from_project(&project).is_err());
    }
}

#[test]
fn appearance_projects_proven_form_genetic_and_current_outfit_paths_losslessly() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let facial = join(&[
        blob(1, &join(&[vint(11), vint(12)])),
        blob(2, &join(&[integer(1, 101), float(2, 0.4)])),
        blob(3, &join(&[integer(1, 102), float(2, -0.2)])),
        blob(4, &join(&[integer(1, 103), float(2, 0.1)])),
        blob(990, b"unknown facial"),
    ]);
    let genetic_part = join(&[
        integer(1, 456),
        integer(2, 8),
        integer(3, 20),
        integer(4, 30),
        integer(5, 40),
        blob(991, b"unknown genetic part"),
    ]);
    let genetics = join(&[
        blob(1, &facial),
        blob(2, b"genetic physique unchanged"),
        blob(5, &blob(1, &genetic_part)),
        blob(6, &blob(1, &genetic_part)),
    ]);
    let occult = join(&[
        integer(1, 4),
        blob(11, b"occult physique unchanged"),
        blob(12, &facial),
        integer(16, 200),
        blob(17, &genetics),
        blob(21, &blob(1, &outfit(true))),
        float(22, -0.3),
        blob(992, b"unknown form"),
    ]);
    let tracker = join(&[
        integer(1, 4),
        integer(2, 4),
        blob(3, &occult),
        integer(5, 1),
    ]);
    let sim = join(&[
        fixed(1, 123),
        integer(7, 2),
        integer(8, 32),
        integer(10, 100),
        blob(12, b"native physique untouched"),
        blob(18, &facial),
        blob(
            21,
            &join(&[blob(1, &outfit(true)), blob(1, &outfit(false))]),
        ),
        blob(28, &genetics),
        blob(30, &blob(17, &tracker)),
        integer(38, 0),
        integer(39, 1),
        integer(60, 1),
        integer(62, 300),
        blob(63, &blob(1, &integer(1, 400))),
        float(67, 0.2),
        blob(993, b"unknown Sim"),
    ]);
    fs::write(
        root.join(HOUSE),
        household_with(&family_with_sim(&sim), &image_section()),
    )
    .unwrap();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("appearance")).unwrap();
    let draft = ProjectDraft::from_project(&project).unwrap();
    let appearance = draft.appearance().unwrap();
    let s = &appearance[0];
    assert_eq!(s.sim_id, 123);
    assert_eq!(s.extended_species, Some(1));
    assert_eq!(s.base.skin_tone, Some(100));
    assert_eq!(s.base.skin_tone_value_shift, Some(0.2));
    assert_eq!(s.base.current_outfit_match, Some(1));
    assert_eq!(s.base.outfits[1].category_index, 1);
    assert_eq!(s.current_occult_types, Some(4));
    assert_eq!(s.occult_forms.len(), 1);
    assert_eq!(s.occult_forms[0].skin_tone, Some(200));
    assert_eq!(s.occult_forms[0].skin_tone_value_shift, Some(-0.3));
    assert_eq!(s.occult_forms[0].current_outfit_match, None);
    let f = s.base.facial.as_ref().unwrap();
    assert_eq!(f.sculpts, vec![11, 12]);
    assert_eq!(f.face_modifiers[0].key, Some(101));
    assert_eq!(f.body_modifiers[0].amount, Some(-0.2));
    assert_eq!(f.aged_face_modifiers[0].key, Some(103));
    assert_eq!(s.base.genetics.as_ref().unwrap().parts[0].id, 456);
    assert_eq!(
        s.base.genetics.as_ref().unwrap().growth_parts[0].body_type,
        8
    );
    assert_eq!(s.base.native.bytes, sim);
    let reconstructed = s
        .base
        .native
        .fields
        .iter()
        .flat_map(|f| s.base.native.encoded(f).unwrap())
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(reconstructed, sim);
    assert!(s.base.native.fields.iter().any(|f| f.number == 993));
    assert_eq!(s.occult_forms[0].native.bytes, occult);
    assert_eq!(f.native.bytes, facial);
}

#[test]
fn absent_current_outfit_is_not_replaced_by_a_default_preset() {
    let fixture = Fixture::new();
    let root = fixture.library();
    let group = scan_library(&root).unwrap().groups.remove(0);
    let project = create_project(&group, &fixture.root.join("appearance")).unwrap();
    let draft = ProjectDraft::from_project(&project).unwrap();
    let a = draft.appearance().unwrap();
    assert_eq!(a[0].current_outfit_category, None);
    assert_eq!(a[0].current_outfit_index, None);
    assert_eq!(a[0].base.current_outfit_match, None);
    assert_eq!(a[0].base.physique, None);
    assert!(a[0].occult_forms.is_empty());
}
