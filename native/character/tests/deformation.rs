use external_cas_assets::{rig::Bone, Catalog, Geometry, ResourceKey, Rig, Vertex};
use external_cas_character::*;
use external_cas_morph::*;
use external_cas_tray::{OutfitPartAppearance, SimAppearance};
use std::sync::{atomic::AtomicBool, Arc};
fn map(kind: u8, delta: [f32; 3]) -> Dmap {
    Dmap {
        version: 7,
        width: 4,
        height: 4,
        age_gender: 0x1010,
        species: Some(1),
        physique: 0,
        kind,
        min_col: 0,
        max_col: 0,
        min_row: 0,
        max_row: 0,
        robe_channel: 0,
        skin_min: 0.,
        skin_delta: 0.,
        robe_min: 0.,
        robe_delta: 0.,
        skin: vec![delta],
        robe: vec![delta],
        raw: vec![],
        has_data: true,
    }
}
fn operation(id: u64, source: &str, data: MorphData) -> MorphOperation {
    let typ = match &data {
        MorphData::Bgeo(_) => 0x067caa11,
        MorphData::Dmap(_) => 0xdb43e069,
        MorphData::Bond(_) => 0x0355e0a6,
    };
    MorphOperation {
        source: source.into(),
        native_amount: Some(1.),
        modifier_key: None,
        weight: Some(1.),
        region: 99,
        resource: MorphResource {
            key: ResourceKey::new(typ, 0, id),
            selected: None,
            provider_sha256: vec![],
            selection: "fabricated".into(),
            raw: None,
            error: None,
        },
        data: Some(data),
    }
}
fn fixture() -> CharacterPlan {
    let native:SimAppearance=serde_json::from_value(serde_json::json!({"sim_index":0,"sim_id":1,"age":16,"gender":4096,"extended_species":1,"base":{"outfits":[],"native":{"bytes":[],"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","fields":[]}},"occult_forms":[]})).unwrap();
    let mut plan = assemble_character(
        &native,
        &Catalog::default(),
        None,
        &CharacterOptions::default(),
    )
    .unwrap();
    let key = ResourceKey::new(0x015a1849, 0, 99);
    let geometry = Arc::new(Geometry {
        version: 12,
        vertices: vec![Vertex {
            position: [1., 0., 0.],
            normal: [0., 1., 0.],
            uv: [0., 0.],
            joints: [0; 4],
            weights: [1., 0., 0., 0.],
            raw_weights: [255, 0, 0, 0],
            uvs: vec![[0., 0.], [0.125, 0.125]],
            has_skin: true,
            vertex_id: Some(10),
            tag: Some(0x00004000),
            tangent: None,
        }],
        indices: vec![],
        bone_palette: vec![1],
        palette_error: None,
        shader: 0,
        merge_group: 0,
        sort_order: 0,
        references: vec![],
        material: vec![],
        unparsed_vertex_usages: vec![],
        uv_stitches: Some(vec![]),
        seam_stitches: Some(vec![]),
        original: vec![0xde, 0xad, 0xbe, 0xef],
    });
    let source = Arc::new(ResourceObservation {
        key,
        candidates: vec![],
        selected: None,
        selection: "fabricated".into(),
        effective_policy: None,
        payload_sha256: None,
        bytes: None,
        error: None,
    });
    plan.parts.push(CharacterPart {
        native_slot: OutfitPartAppearance {
            part_index: 0,
            cas_part_id: 1,
            body_type: 3,
            color_shift: None,
        },
        casp_candidates: vec![],
        source: None,
        original_source: None,
        substitution: None,
        substitution_unresolved: false,
        reference_selection: PartSelection::IncludedByReference,
        selected_lod: Some(0),
        casp: None,
        meshes: vec![CharacterMesh {
            key,
            source,
            native_rest: Some(geometry),
            morphed_rest: None,
            native_rig_binding: Some(vec![[0; 4]]),
            regions: vec![],
            visibility: MeshVisibility::VisibleByRegionRule,
            issues: vec![],
        }],
        materials: vec![],
        region_source: None,
        region_map: None,
        issues: vec![],
    });
    plan.rig = Some(Rig {
        bones: vec![Bone {
            name: "root".into(),
            opposing_index: -1,
            parent_index: -1,
            hash: 1,
            flags: 0,
            position: [0.; 3],
            orientation: [0., 0., 0., 1.],
            scale: [1.; 3],
        }],
        chains: vec![],
    });
    let bgeo = Bgeo {
        version: 0x600,
        public_keys: vec![],
        lods: vec![BgeoLod {
            index_base: 10,
            vertices: 1,
            vectors: 1,
        }],
        blends: vec![BgeoBlend {
            position: true,
            normal: false,
            offset: 0,
            index: 0,
        }],
        vectors: vec![[-2., 0., 0.]],
        raw: vec![],
        unparsed_suffix: vec![],
    };
    let bond = Bond {
        version: 1,
        adjustments: vec![BoneAdjustment {
            slot_hash: 1,
            offset: [0.5, 0., 0.],
            scale: [0.; 3],
            quaternion: [0., 0., 0., 1.],
        }],
        raw: vec![],
        unparsed_suffix: vec![],
    };
    // Intentionally interleaved resource order: stage ordering must be explicit.
    plan.morph.as_mut().unwrap().operations = vec![
        operation(2, "map", MorphData::Dmap(map(0, [1., 0., 0.]))),
        operation(4, "bond", MorphData::Bond(bond)),
        operation(1, "bgeo", MorphData::Bgeo(bgeo)),
        operation(3, "map", MorphData::Dmap(map(1, [0., 1., 0.]))),
    ];
    plan
}
#[test]
fn stage_order_keeps_native_original_mirror_positions_and_updates_rig_once() {
    let plan = fixture();
    let original = plan.parts[0].meshes[0].native_rest.as_ref().unwrap();
    let out = build_reference_deformation(
        &plan,
        &ReferenceDeformOptions::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(out.meshes[0].geometry.vertices[0].position, [-1.5, 0., 0.]);
    assert_eq!(out.meshes[0].geometry.vertices[0].normal, [0.; 3]);
    assert_eq!(out.rig.unwrap().bones[0].position, [0.5, 0., 0.]);
    assert_eq!(out.meshes[0].geometry.original, original.original);
    assert_eq!(original.vertices[0].position, [1., 0., 0.]);
    assert_eq!(
        out.meshes[0]
            .operations
            .iter()
            .map(|o| o.operation_index)
            .collect::<Vec<_>>(),
        [2, 0, 1]
    );
    assert!(!out.native_qualified);
}
#[test]
fn work_budget_missing_ids_and_stale_revision_are_not_silently_accepted() {
    let mut plan = fixture();
    let out = build_reference_deformation(
        &plan,
        &ReferenceDeformOptions {
            max_vertex_operations: 1,
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(out.vertex_operations, 1);
    assert_eq!(
        out.meshes[0]
            .operations
            .iter()
            .filter(|o| matches!(o.state, ReferenceOperationState::Unresolved(_)))
            .count(),
        2
    );
    let g = Arc::make_mut(plan.parts[0].meshes[0].native_rest.as_mut().unwrap());
    g.vertices[0].vertex_id = None;
    g.vertices[0].tag = Some(0x003f4000);
    let out = build_reference_deformation(
        &plan,
        &ReferenceDeformOptions::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(matches!(
        out.meshes[0].operations[0].state,
        ReferenceOperationState::Unresolved(_)
    ));
    assert!(matches!(
        build_reference_deformation(
            &plan,
            &ReferenceDeformOptions::default(),
            &AtomicBool::new(true),
            |_| {}
        ),
        Err(CharacterError::Cancelled)
    ));
    plan.scene.native_sim_sha256 = "stale".into();
    assert!(build_reference_deformation(
        &plan,
        &ReferenceDeformOptions::default(),
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
}
#[test]
fn region_rules_are_literal_reference_predicates() {
    assert!(reference_region_allows(60, 9));
    assert!(!reference_region_allows(3, 9));
    assert!(reference_region_allows(61, 31));
    assert!(!reference_region_allows(6, 31));
    assert!(reference_region_allows(3, 99));
    assert!(reference_region_allows(4, 14));
}
