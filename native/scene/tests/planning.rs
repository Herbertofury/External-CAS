use external_cas_assets::{Catalog, ResourceKey};
use external_cas_scene::*;
use external_cas_tray::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
fn tone(version: u32) -> Vec<u8> {
    let mut b = version.to_le_bytes().to_vec();
    if version >= 10 {
        b.push(1);
        b.extend(200u64.to_le_bytes());
        b.extend(201u64.to_le_bytes());
        for f in [0.4f32, 0.5, 0.6] {
            b.extend(f.to_le_bytes());
        }
    } else {
        b.extend(200u64.to_le_bytes());
    }
    b.extend(1i32.to_le_bytes());
    b.extend(0x2020u32.to_le_bytes());
    b.extend(202u64.to_le_bytes());
    b.extend(10u16.to_le_bytes());
    b.extend(20u16.to_le_bytes());
    b.extend(75u32.to_le_bytes());
    b.extend(1i32.to_le_bytes());
    b.extend(4u16.to_le_bytes());
    if version >= 7 {
        b.extend(5u32.to_le_bytes());
    } else {
        b.extend(5u16.to_le_bytes());
    }
    if version < 10 {
        b.extend(0.5f32.to_le_bytes());
    }
    b.push(1);
    b.extend(0xff123456u32.to_le_bytes());
    b.extend(2.5f32.to_le_bytes());
    if version < 10 {
        b.extend(0.6f32.to_le_bytes());
    }
    if version >= 8 {
        b.extend(333u64.to_le_bytes());
    }
    if version > 10 {
        b.extend(2u16.to_le_bytes());
        for f in [-1f32, 1.0, 0.1] {
            b.extend(f.to_le_bytes());
        }
    }
    b
}
fn sim() -> SimAppearance {
    serde_json::from_value(serde_json::json!({
    "sim_index":0,"sim_id":1,"gender":8192,"age":32,"extended_species":1,"current_outfit_category":0,"current_outfit_index":0,"custom_texture":null,"pelt_layers":null,"occult_types":null,"current_occult_types":null,"occult_form_available":null,
    "base":{"occult_form_index":null,"occult_type":null,"skin_tone":100,"skin_tone_value_shift":0.25,"physique":null,"facial":null,"genetics":null,"outfits":[{"outfit_index":0,"outfit_id":44,"category":0,"category_index":0,"parts":[{"part_index":0,"cas_part_id":777,"body_type":8,"color_shift":123}],"native":{"bytes":[],"sha256":"outfit","fields":[]}}],"current_outfit_match":0,"native":{"bytes":[],"sha256":"native","fields":[]}},"occult_forms":[]
})).unwrap()
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
            "scene-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn package(&self, name: &str, resources: &[(ResourceKey, Vec<u8>)]) -> PathBuf {
        let mut b = vec![0u8; 96];
        b[..4].copy_from_slice(b"DBPF");
        b[4..8].copy_from_slice(&2u32.to_le_bytes());
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[36..40].copy_from_slice(&(resources.len() as u32).to_le_bytes());
        let mut index = 0u32.to_le_bytes().to_vec();
        for (key, payload) in resources {
            let offset = b.len() as u32;
            b.extend(payload);
            for n in [
                key.type_id,
                key.group_id,
                (key.instance_id >> 32) as u32,
                key.instance_id as u32,
                offset,
                0x80000000 | payload.len() as u32,
                payload.len() as u32,
            ] {
                index.extend(n.to_le_bytes());
            }
            index.extend(0u16.to_le_bytes());
            index.extend(1u16.to_le_bytes());
        }
        let offset = b.len() as u32;
        b[44..48].copy_from_slice(&(index.len() as u32).to_le_bytes());
        b[64..68].copy_from_slice(&offset.to_le_bytes());
        b.extend(index);
        let path = self.root.join(name);
        fs::write(&path, b).unwrap();
        path
    }
}
// Preserve generated fixtures for inspection. Disk cleanup is an explicit
// user-controlled operation; tests do not recursively delete their folders.

#[test]
fn rig_hash_vectors_match_compiled_original_fnv_source() {
    // Direct Add-Type execution of pinned original FNV.cs, recorded 2026-11-12.
    for (name, expected) in [
        ("auRig", 0xd057fcc534c1bcbb),
        ("iuRig", 0xe4d84c831f5a9183),
        ("puRig", 0x192f5c47f5d28a72),
        ("cuRig", 0x2183c4b327c38fc9),
        ("nuRig", 0xb4d964596788d27c),
        ("adRig", 0x52385bc57e4db284),
        ("alRig", 0x0d5a13c5576b847c),
    ] {
        assert_eq!(fnv64(name).unwrap(), expected);
    }
    for age in [8, 16, 32, 64] {
        assert_eq!(
            native_rig_key(Some(1), Some(age), 1).unwrap().0.instance_id,
            0xd057fcc534c1bcbb
        );
    }
    assert_eq!(native_rig_key(Some(1), Some(128), 1).unwrap().1, "iuRig");
    assert_eq!(native_rig_key(Some(4), Some(4), 1).unwrap().1, "cdRig");
    assert_eq!(
        native_rig_key(Some(1), Some(32), 32).unwrap().0.instance_id,
        0x60faa42f9b0b4e39
    );
    assert!(native_rig_key(Some(99), Some(32), 1).is_err());
    assert!(native_rig_key(Some(1), None, 1).is_err());
    assert!(native_rig_key(Some(1), Some(32), 6).is_err());
}
#[test]
fn tone_versions_truncation_and_unknown_suffix_are_preserved() {
    for v in [6, 7, 8, 10, 11, 12] {
        let raw = tone(v);
        let parsed = Tone::parse(&raw).unwrap();
        assert_eq!(parsed.skin_sets[0].texture_instance, 200);
        assert_eq!(parsed.skin_sets[0].makeup_opacity, 0.5);
        assert_eq!(parsed.tags, vec![(4, 5)]);
        assert_eq!(parsed.raw, raw);
        for n in 0..raw.len() {
            assert!(Tone::parse(&raw[..n]).is_err(), "version{v} truncation{n}");
        }
    }
    let mut raw = tone(12);
    raw.extend(b"new parameters");
    assert_eq!(
        Tone::parse(&raw).unwrap().uninterpreted_suffix,
        b"new parameters"
    );
    assert!(matches!(
        Tone::parse(&99u32.to_le_bytes()),
        Err(ToneError::UnsupportedVersion(99))
    ));
    // This fabricated legacy-shaped v9 previously mirrored an incorrect
    // reference assumption. Native v9 uses different skin-set framing.
    assert!(matches!(Tone::parse(&tone(9)),Err(ToneError::UnsupportedVersion(9))));
    let mut bad = tone(11);
    bad[21..25].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(Tone::parse(&bad).is_err());
}
#[test]
fn actual_full_keys_are_preserved_and_duplicates_remain_ambiguous() {
    let fixture = Fixture::new();
    let rig = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let resources = vec![
        (rig, vec![1]),
        (ResourceKey::new(TONE, 0, 100), tone(11)),
        (ResourceKey::new(0x034aeecb, 0x1234, 777), vec![1]),
        (ResourceKey::new(0x2bc04edf, 0, 200), vec![1]),
        (ResourceKey::new(0x3453cf95, 0, 201), vec![1]),
    ];
    let path = fixture.package("base.package", &resources);
    let catalog = Catalog::scan_roots(&[path.clone()], &AtomicBool::new(false), |_| {}).unwrap();
    let options = SceneOptions::default();
    let plan = resolve_scene(&sim(), &catalog, &options);
    assert_eq!(
        plan.rig.as_ref().unwrap().state,
        DependencyState::UniqueInScope
    );
    assert_eq!(
        plan.outfit[0]
            .resource
            .selected
            .as_ref()
            .unwrap()
            .key
            .group_id,
        0x1234
    );
    assert_eq!(plan.tone.as_ref().unwrap().selected_skin_set, Some(0));
    assert_eq!(plan.tone.as_ref().unwrap().value_shift, Some(0.25));
    assert!(!plan.complete_sim);
    let duplicate = fixture.package(
        "override.package",
        &[
            (rig, vec![2]),
            (ResourceKey::new(0x034aeecb, 0x4321, 777), vec![2]),
        ],
    );
    let catalog = Catalog::scan_roots(&[path, duplicate], &AtomicBool::new(false), |_| {}).unwrap();
    let plan = resolve_scene(&sim(), &catalog, &options);
    assert_eq!(plan.rig.as_ref().unwrap().state, DependencyState::Ambiguous);
    assert_eq!(plan.outfit[0].resource.state, DependencyState::Ambiguous);
    assert_eq!(plan.outfit[0].resource.requested_keys.len(), 2);
    let mut explicit = SceneOptions::default();
    let provider = catalog.providers(rig)[0];
    explicit.provider_choices.insert(rig, provider);
    let plan = resolve_scene(&sim(), &catalog, &explicit);
    assert_eq!(
        plan.rig.as_ref().unwrap().state,
        DependencyState::ExplicitProvider
    );
}
#[test]
fn absent_current_selection_does_not_render_an_arbitrary_outfit() {
    let mut sim = sim();
    sim.base.current_outfit_match = None;
    let plan = resolve_scene(&sim, &Catalog::default(), &SceneOptions::default());
    assert!(plan.outfit.is_empty());
    assert!(plan.failures.iter().any(|f| f.role == "current outfit"));
}

#[test]
#[ignore = "requires explicit original source oracle; not a fixture-only check"]
fn source_tone_reader_oracle() {
    let source = std::env::var_os("CAS_SCENE_TONE_SOURCE")
        .expect("CAS_SCENE_TONE_SOURCE is required for explicit source-oracle comparison");
    let fixture = Fixture::new();
    let path = fixture.root.join("tone11.bin");
    fs::write(&path, tone(11)).unwrap();
    let output = std::process::Command::new("pwsh")
        .args([
            "-NoProfile",
            "-File",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tone-source-oracle.ps1"),
            "-ToneSource",
        ])
        .arg(source)
        .arg("-InputFile")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let j: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let parsed = Tone::parse(&fs::read(path).unwrap()).unwrap();
    assert_eq!(j["version"], parsed.version);
    assert_eq!(
        j["texture"],
        format!("{:016X}", parsed.skin_sets[0].texture_instance)
    );
    assert_eq!(
        j["overlay"],
        format!("{:016X}", parsed.skin_sets[0].overlay_instance)
    );
    assert_eq!(j["hue"], parsed.hue);
    assert_eq!(j["saturation"], parsed.saturation);
    assert_eq!(j["opacity"], parsed.opacity);
    assert_eq!(j["overlayCount"], parsed.overlays.len());
    assert_eq!(
        j["makeupOpacity"].as_f64().unwrap(),
        parsed.skin_sets[0].makeup_opacity as f64
    );
}

#[test]
fn identical_full_tgi_providers_resolve_by_bytes_and_keep_all_hashes() {
    let fixture = Fixture::new();
    let rig = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let casp = ResourceKey::new(0x034aeecb, 0, 777);
    let a = fixture.package("a.package", &[(rig, vec![1, 2, 3]), (casp, vec![4, 5, 6])]);
    let b = fixture.package("b.package", &[(rig, vec![1, 2, 3]), (casp, vec![4, 5, 6])]);
    let catalog = Catalog::scan_roots(&[a, b], &AtomicBool::new(false), |_| {}).unwrap();
    let plan = resolve_scene(&sim(), &catalog, &SceneOptions::default());
    let rig = plan.rig.unwrap();
    assert_eq!(rig.state, DependencyState::ByteIdenticalProviders);
    assert_eq!(rig.candidates.len(), 2);
    assert_eq!(rig.provider_payload_hashes.len(), 2);
    assert_eq!(
        rig.provider_payload_hashes[0].1,
        rig.provider_payload_hashes[1].1
    );
    assert_eq!(plan.rig_payload.unwrap(), vec![1, 2, 3]);
    assert_eq!(
        plan.outfit[0].resource.state,
        DependencyState::ByteIdenticalProviders
    );
}

#[test]
fn native_suntan_state_selects_serialized_skinset_and_wrong_wire_never_defaults() {
    let fixture = Fixture::new();
    let mut raw = tone(11);
    raw[4] = 3;
    let additional = raw[5..33].repeat(2);
    raw.splice(33..33, additional);
    let path = fixture.package("tone.package", &[(ResourceKey::new(TONE, 0, 100), raw)]);
    let catalog = Catalog::scan_roots(&[path], &AtomicBool::new(false), |_| {}).unwrap();
    let mut sim = sim();
    // SimData30 -> attributes27 -> suntan1 varint2.
    sim.base.native.bytes = vec![0xf2, 1, 5, 0xda, 1, 2, 8, 2];
    let plan = resolve_scene(&sim, &catalog, &SceneOptions::default());
    assert_eq!(plan.tone.unwrap().selected_skin_set, Some(2));
    // A changed known wire is unsupported, rather than becoming untanned.
    sim.base.native.bytes = vec![0xf2, 1, 6, 0xda, 1, 3, 10, 1, 2];
    let plan = resolve_scene(&sim, &catalog, &SceneOptions::default());
    assert_eq!(plan.tone.unwrap().selected_skin_set, None);
}

#[test]
fn changed_or_invalid_providers_never_yield_a_rig_payload() {
    let fixture = Fixture::new();
    let rig = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let path = fixture.package("rig.package", &[(rig, vec![1, 2, 3])]);
    let catalog = Catalog::scan_roots(&[path.clone()], &AtomicBool::new(false), |_| {}).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[96] ^= 1;
    fs::write(&path, bytes).unwrap();
    // Exercise an observable metadata change deterministically. Same-stamp
    // payload rewrites require a retained payload pin, separately tested by
    // assets; filesystem timestamp resolution is not a content guarantee.
    let changed_time =
        fs::metadata(&path).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(changed_time))
        .unwrap();
    let plan = resolve_scene(&sim(), &catalog, &SceneOptions::default());
    assert!(plan.rig_payload.is_none());
    assert!(plan.failures.iter().any(|f| f.role == "native rig payload"));
    let mut catalog = Catalog::default();
    catalog.by_key.insert(
        rig,
        vec![external_cas_assets::scan::Provider {
            package_index: 99,
            entry_index: 0,
        }],
    );
    let plan = resolve_scene(&sim(), &catalog, &SceneOptions::default());
    assert_eq!(plan.rig.unwrap().state, DependencyState::InvalidProvider);
    assert!(plan.rig_payload.is_none());
}

fn install_fixture(fixture: &Fixture) -> PathBuf {
    let root = fixture.root.join("install");
    for p in [
        root.join("Data/Client"),
        root.join("Data/Simulation"),
        root.join("Game/Bin"),
    ] {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(root.join("Data/Client/Resource.cfg"),b"Priority -20\nPackedFile ClientDeltaBuild*.package\nPriority -30\nPackedFile ClientFullBuild*.package\n").unwrap();
    fs::write(
        root.join("Data/Simulation/Resource.cfg"),
        b"Priority -30\nPackedFile SimulationFullBuild*.package\n",
    )
    .unwrap();
    fs::write(
        root.join("Game/Bin/Default.ini"),
        b"[Version]\ngameversion = 1.129.39.1030\n",
    )
    .unwrap();
    root
}
#[test]
fn scoped_identical_highest_ties_ignore_different_history_but_hash_every_provider() {
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    for different in [false, true] {
        let fixture = Fixture::new();
        let root = install_fixture(&fixture);
        fixture.package(
            "install/Data/Client/ClientFullBuild0.package",
            &[(key, b"old".to_vec())],
        );
        fixture.package(
            "install/Data/Client/ClientDeltaBuild0.package",
            &[(key, b"new".to_vec())],
        );
        fixture.package(
            "install/Data/Client/ClientDeltaBuild1.package",
            &[(
                key,
                if different {
                    b"bad".to_vec()
                } else {
                    b"new".to_vec()
                },
            )],
        );
        let catalog =
            Catalog::scan_roots(&[root.clone()], &AtomicBool::new(false), |_| {}).unwrap();
        let scope = GameInstallScope::discover(&root).unwrap();
        let policy = scope.prepare(&catalog).unwrap();
        let plan = resolve_scene_scoped(&sim(), &catalog, &SceneOptions::default(), Some(&policy));
        let dependency = plan.rig.as_ref().unwrap();
        assert_eq!(dependency.provider_payload_hashes.len(), 3);
        if different {
            assert!(plan.rig_payload.is_none());
            assert_eq!(dependency.state, DependencyState::Ambiguous);
        } else {
            assert_eq!(plan.rig_payload.as_deref(), Some(b"new".as_slice()));
            assert_eq!(
                dependency.state,
                DependencyState::ByteIdenticalPriorityProviders
            );
        }
        assert!(resolve_scene(&sim(), &catalog, &SceneOptions::default())
            .rig_payload
            .is_none());
    }
}
#[test]
fn scoped_reader_never_revives_lower_history_after_highest_deletion() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    fixture.package(
        "install/Data/Client/ClientFullBuild0.package",
        &[(key, b"old".to_vec())],
    );
    let patch = fixture.package(
        "install/Data/Client/ClientDeltaBuild0.package",
        &[(key, b"new".to_vec())],
    );
    let mut bytes = fs::read(&patch).unwrap();
    let index = u32::from_le_bytes(bytes[64..68].try_into().unwrap()) as usize;
    bytes[index + 32..index + 34].copy_from_slice(&0xffe0u16.to_le_bytes());
    fs::write(patch, bytes).unwrap();
    let catalog = Catalog::scan_roots(&[root.clone()], &AtomicBool::new(false), |_| {}).unwrap();
    let scope = GameInstallScope::discover(&root).unwrap();
    let policy = scope.prepare(&catalog).unwrap();
    let plan = resolve_scene_scoped(&sim(), &catalog, &SceneOptions::default(), Some(&policy));
    assert!(plan.rig_payload.is_none());
    assert_eq!(plan.rig.unwrap().state, DependencyState::Deleted);
}
#[test]
fn explicit_numeric_game_priorities_choose_delta_and_retain_lower_providers() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let delta = fixture.package(
        "install/Data/Client/ClientDeltaBuild0.package",
        &[(key, vec![1])],
    );
    let full = fixture.package(
        "install/Data/Client/ClientFullBuild0.package",
        &[(key, vec![2])],
    );
    let simulation = fixture.package(
        "install/Data/Simulation/SimulationFullBuild0.package",
        &[(key, vec![2])],
    );
    let scope = GameInstallScope::discover(&root).unwrap();
    assert_eq!(
        scope.observed_game_version.as_deref(),
        Some("1.129.39.1030")
    );
    assert_eq!(scope.packages.len(), 3);
    let catalog = Catalog::scan_roots(
        &[full, simulation, delta.clone()],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let policy = scope.prepare(&catalog).unwrap();
    let decision = policy.resolve_key(key);
    assert_eq!(decision.state, EffectiveState::UniqueHighestPriority);
    assert_eq!(decision.candidates.len(), 3);
    let chosen = decision.selected.unwrap();
    assert_eq!(
        catalog.provider(chosen).unwrap().0.path,
        delta.canonicalize().unwrap()
    );
    assert!(decision.configured_scope_complete);
    assert!(!policy.coverage.active_pack_entitlements_verified);
    let choices = policy.provider_choices_for_keys([key]);
    assert_eq!(choices.choices.get(&key), Some(&chosen));
    assert_eq!(
        choices.decisions[0]
            .candidates
            .iter()
            .map(|c| c.priority.unwrap())
            .max(),
        Some(-20)
    );
    assert!(choices.decisions[0]
        .candidates
        .iter()
        .all(|c| !c.evidence.is_empty()));
}
#[test]
fn equal_priority_and_unconfigured_cc_are_not_alphabetically_guessed() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let a = fixture.package(
        "install/Data/Client/ClientDeltaBuild0.package",
        &[(key, vec![1])],
    );
    let b = fixture.package(
        "install/Data/Client/ClientDeltaBuild1.package",
        &[(key, vec![2])],
    );
    let scope = GameInstallScope::discover(&root).unwrap();
    let catalog =
        Catalog::scan_roots(&[a.clone(), b.clone()], &AtomicBool::new(false), |_| {}).unwrap();
    let policy = scope.prepare(&catalog).unwrap();
    assert_eq!(
        policy.resolve_key(key).state,
        EffectiveState::TiedHighestPriority
    );
    assert!(policy.provider_choices_for_keys([key]).choices.is_empty());
    let cc = fixture.package("some-creator.package", &[(key, vec![3])]);
    let catalog = Catalog::scan_roots(&[a, b, cc], &AtomicBool::new(false), |_| {}).unwrap();
    let policy = scope.prepare(&catalog).unwrap();
    assert_eq!(
        policy.resolve_key(key).state,
        EffectiveState::UnconfiguredProvider
    );
    assert_eq!(policy.coverage.unscoped_catalog_paths.len(), 1);
    assert!(!policy.coverage.configured_scope_complete());
}
#[test]
fn config_version_and_matching_package_census_changes_invalidate_policy() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let package = fixture.package(
        "install/Data/Client/ClientFullBuild0.package",
        &[(key, vec![1])],
    );
    let scope = GameInstallScope::discover(&root).unwrap();
    let catalog = Catalog::scan_roots(&[package], &AtomicBool::new(false), |_| {}).unwrap();
    fs::write(
        root.join("Game/Bin/Default.ini"),
        b"[Version]\ngameversion=updated\n",
    )
    .unwrap();
    assert!(scope.prepare(&catalog).is_err());
    let scope = GameInstallScope::discover(&root).unwrap();
    fixture.package(
        "install/Data/Client/ClientDeltaBuild0.package",
        &[(key, vec![2])],
    );
    assert!(scope.prepare(&catalog).is_err());
    let scope = GameInstallScope::discover(&root).unwrap();
    let policy = scope.prepare(&catalog).unwrap();
    assert_eq!(policy.coverage.unindexed_paths.len(), 1);
    assert!(!policy.coverage.configured_scope_complete());
    fs::write(
        root.join("Data/Client/Resource.cfg"),
        b"Priority 500\nPackedFile *.package\n",
    )
    .unwrap();
    assert!(scope.prepare(&catalog).is_err());
}
#[test]
fn unknown_conditions_never_sneak_a_high_priority_override_into_scope() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    fs::write(root.join("Data/Client/Resource.cfg"),b"Priority -30\nPackedFile ClientFullBuild*.package\nSelect UNKNOWN_RUNTIME_CONDITION\nPriority 900\nPackedFile ClientDeltaBuild*.package\nEnd\n").unwrap();
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let full = fixture.package(
        "install/Data/Client/ClientFullBuild0.package",
        &[(key, vec![1])],
    );
    fixture.package(
        "install/Data/Client/ClientDeltaBuild0.package",
        &[(key, vec![2])],
    );
    let scope = GameInstallScope::discover(&root).unwrap();
    assert_eq!(scope.packages.len(), 1);
    assert_eq!(scope.issues.len(), 1);
    let catalog = Catalog::scan_roots(&[full], &AtomicBool::new(false), |_| {}).unwrap();
    assert!(!scope
        .prepare(&catalog)
        .unwrap()
        .coverage
        .configured_scope_complete());
}

#[test]
fn conditional_overlap_and_unsupported_directives_disable_automatic_selection() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let full = fixture.package(
        "install/Data/Client/ClientFullBuild0.package",
        &[(key, vec![1])],
    );
    fs::write(root.join("Data/Client/Resource.cfg"),b"Priority -30\nPackedFile ClientFullBuild*.package\nSelect UNKNOWN\nPriority 900\nPackedFile ClientFullBuild*.package\nEnd\n").unwrap();
    let scope = GameInstallScope::discover(&root).unwrap();
    assert!(scope.packages.is_empty());
    let catalog = Catalog::scan_roots(&[full], &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(
        scope.prepare(&catalog).unwrap().resolve_key(key).state,
        EffectiveState::UnconfiguredProvider
    );
    fs::write(
        root.join("Data/Client/Resource.cfg"),
        b"Priority -30\nPackedFile ClientFullBuild*.package\nUnknownPriorityMode 900\n",
    )
    .unwrap();
    assert!(GameInstallScope::discover(&root)
        .unwrap()
        .packages
        .is_empty());
}
#[test]
fn highest_priority_tombstone_does_not_restore_lower_resource() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    let full = fixture.package(
        "install/Data/Client/ClientFullBuild0.package",
        &[(key, vec![1])],
    );
    let delta = fixture.package(
        "install/Data/Client/ClientDeltaBuild0.package",
        &[(key, vec![2])],
    );
    let mut bytes = fs::read(&delta).unwrap();
    let n = bytes.len();
    bytes[n - 4..n - 2].copy_from_slice(&0xffe0u16.to_le_bytes());
    fs::write(&delta, bytes).unwrap();
    let scope = GameInstallScope::discover(&root).unwrap();
    let catalog = Catalog::scan_roots(&[full, delta], &AtomicBool::new(false), |_| {}).unwrap();
    let p = scope.prepare(&catalog).unwrap();
    assert_eq!(
        p.resolve_key(key).state,
        EffectiveState::DeletedHighestPriority
    );
    assert!(p.provider_choices_for_keys([key]).choices.is_empty());
}
#[test]
fn multiple_highest_priority_tombstones_are_absent_but_mixed_live_ties_stay_unresolved(){
    let fixture=Fixture::new();let root=install_fixture(&fixture);let key=native_rig_key(Some(1),Some(32),1).unwrap().0;
    fs::write(root.join("Data/Simulation/Resource.cfg"),b"Priority -20\nPackedFile SimulationDeltaBuild*.package\nPriority -30\nPackedFile SimulationFullBuild*.package\n").unwrap();
    let full=fixture.package("install/Data/Client/ClientFullBuild0.package",&[(key,vec![1])]);let client=fixture.package("install/Data/Client/ClientDeltaBuild0.package",&[(key,vec![2])]);let simulation=fixture.package("install/Data/Simulation/SimulationDeltaBuild0.package",&[(key,vec![3])]);
    for path in [&client,&simulation]{let mut bytes=fs::read(path).unwrap();let n=bytes.len();bytes[n-4..n-2].copy_from_slice(&0xffe0u16.to_le_bytes());fs::write(path,bytes).unwrap();}
    let scope=GameInstallScope::discover(&root).unwrap();let catalog=Catalog::scan_roots(&[full.clone(),client.clone(),simulation.clone()],&AtomicBool::new(false),|_|{}).unwrap();let policy=scope.prepare(&catalog).unwrap();let resolution=policy.resolve_key(key);assert_eq!(resolution.state,EffectiveState::DeletedHighestPriority);assert!(resolution.selected.is_none());assert_eq!(resolution.candidates.len(),3);assert!(policy.provider_choices_for_keys([key]).choices.is_empty());
    let _simulation=fixture.package("install/Data/Simulation/SimulationDeltaBuild0.package",&[(key,vec![3])]);let catalog=Catalog::scan_roots(&[full,client,simulation],&AtomicBool::new(false),|_|{}).unwrap();let scope=GameInstallScope::discover(&root).unwrap();let resolution=scope.prepare(&catalog).unwrap().resolve_key(key);assert_eq!(resolution.state,EffectiveState::TiedHighestPriority);assert!(resolution.selected.is_none());
}
#[test]
fn pack_delta_directory_does_not_invent_a_priority_above_its_config() {
    let fixture = Fixture::new();
    let root = install_fixture(&fixture);
    let key = native_rig_key(Some(1), Some(32), 1).unwrap().0;
    for dir in [root.join("EP01"), root.join("Delta/EP01")] {
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("ResourceClient.cfg"),
            b"Priority -30\nPackedFile ClientFullBuild*.package\n",
        )
        .unwrap();
    }
    let pack = fixture.package("install/EP01/ClientFullBuild0.package", &[(key, vec![1])]);
    let delta = fixture.package(
        "install/Delta/EP01/ClientFullBuild0.package",
        &[(key, vec![2])],
    );
    let scope = GameInstallScope::discover(&root).unwrap();
    let catalog = Catalog::scan_roots(&[pack, delta], &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(
        scope.prepare(&catalog).unwrap().resolve_key(key).state,
        EffectiveState::TiedHighestPriority
    );
}
