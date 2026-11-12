use external_cas_assets::{geom::UvStitch, Geometry, Vertex};
use external_cas_assets::{rig::Bone, Rig};
use external_cas_morph::*;

fn put32(b: &mut Vec<u8>, v: u32) {
    b.extend(v.to_le_bytes());
}
fn rcol(body: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [3, 1, 0, 0, 1] {
        put32(&mut b, v);
    }
    b.extend([0; 16]);
    put32(&mut b, 44);
    put32(&mut b, body.len() as u32);
    b.extend(body);
    b
}
fn bgeo() -> Vec<u8> {
    let mut b = b"BGEO".to_vec();
    for v in [0x600, 1, 3, 3, 10, 3, 3] {
        put32(&mut b, v);
    }
    for v in [3i16, 4, 2] {
        b.extend(v.to_le_bytes());
    }
    for v in [[800i16, 0, -800], [0, 800, 0], [400, 0, 0]] {
        for q in v {
            b.extend(((q as u16) ^ 0x8000).to_le_bytes());
        }
    }
    rcol(&b)
}
fn map(kind: u8) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [7, 8, 4, 0x2020, 1] {
        put32(&mut b, v);
    }
    b.extend([0, kind]);
    for v in [0, 3, 0, 3] {
        put32(&mut b, v);
    }
    b.push(2);
    b.extend((-1f32).to_le_bytes());
    b.extend(2f32.to_le_bytes());
    put32(&mut b, 4 * 16);
    for _ in 0..4 {
        b.extend(16u16.to_le_bytes());
        b.extend([0, 2]);
        for _ in 0..4 {
            b.extend([255, 128, 0]);
        }
    }
    b
}
fn geom() -> Geometry {
    Geometry {
        version: 12,
        vertices: (0..4)
            .map(|i| Vertex {
                position: [if i % 2 == 0 { 1.0 } else { -1.0 }, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
                joints: [0; 4],
                weights: [0.0; 4],
                raw_weights: [0; 4],
                uvs: vec![[0.0, 0.0], [0.5, 0.5]],
                has_skin: false,
                vertex_id: Some([10, 10, 11, -1][i]),
                tag: Some(0x003f4000),
                tangent: Some([1.0, 0.0, 0.0]),
            })
            .collect(),
        indices: vec![0, 1, 2],
        bone_palette: vec![],
        palette_error: None,
        shader: 0,
        merge_group: 0,
        sort_order: 0,
        references: vec![],
        material: vec![],
        unparsed_vertex_usages: vec![],
        uv_stitches: Some(vec![]),
        seam_stitches: Some(vec![]),
        original: vec![1, 2, 3],
    }
}
#[test]
fn bgeo_native_ids_duplicates_negative_and_zero() {
    let b = Bgeo::parse(&bgeo()).unwrap();
    let g = geom();
    let a = apply_bgeo(&g, &b, 0, 0.5, false).unwrap();
    assert_eq!(a.geometry.vertices[0].position, [1.05, 0.0, -0.05]);
    assert_eq!(a.geometry.vertices[1].position, [-0.95, 0.0, -0.05]);
    assert_eq!(a.geometry.vertices[2].position, g.vertices[2].position);
    assert_eq!(a.geometry.vertices[3].position, g.vertices[3].position);
    assert_eq!(a.geometry.original, g.original);
    let n = apply_bgeo(&g, &b, 0, -0.5, false).unwrap();
    assert_eq!(n.geometry.vertices[0].position, [0.95, 0.0, 0.05]);
    assert_eq!(
        apply_bgeo(&g, &b, 0, 0.0, false)
            .unwrap()
            .stats
            .matched_vertices,
        0
    );
}
#[test]
fn bgeo_truncation_and_unknown_suffix_guard() {
    let bytes = bgeo();
    for n in 0..bytes.len() {
        assert!(Bgeo::parse(&bytes[..n]).is_err(), "accepted prefix {n}");
    }
    let mut bytes = bytes;
    bytes.push(9);
    let b = Bgeo::parse(&bytes).unwrap();
    assert_eq!(b.unparsed_suffix, [9]);
    assert!(apply_bgeo(&geom(), &b, 0, 1.0, false).is_err());
}
#[test]
fn dmap_mirror_weights_first_seam_and_preservation() {
    let d = Dmap::parse(&map(0)).unwrap();
    let n = Dmap::parse(&map(1)).unwrap();
    let mut g = geom();
    g.uv_stitches = Some(vec![
        UvStitch {
            vertex_index: 0,
            coordinates: vec![[0.25, 0.25]],
        },
        UvStitch {
            vertex_index: 0,
            coordinates: vec![[10.0, 10.0]],
        },
    ]);
    g.vertices[2].tag = Some(0);
    let positions = g.vertices.iter().map(|v| v.position).collect::<Vec<_>>();
    let a = apply_dmap(&g, &positions, &d, 0.5, Some((&n, -0.25))).unwrap();
    assert_eq!(a.geometry.vertices[0].position, [0.5, 0.0, 0.5]);
    assert_eq!(a.geometry.vertices[1].position, [-0.5, 0.0, 0.5]);
    assert_eq!(a.geometry.vertices[0].normal, [0.25, 1.0, -0.25]);
    assert_eq!(a.geometry.vertices[2].position, g.vertices[2].position);
    assert_eq!(a.geometry.original, g.original);
    assert_eq!(a.geometry.vertices[0].tangent, g.vertices[0].tangent);
}
#[test]
fn dmap_zero_negative_combined_and_truncation() {
    let raw = map(0);
    for n in 0..raw.len() {
        assert!(Dmap::parse(&raw[..n]).is_err(), "accepted prefix{n}");
    }
    let d = Dmap::parse(&raw).unwrap();
    let g = geom();
    let p = g.vertices.iter().map(|v| v.position).collect::<Vec<_>>();
    let first = apply_dmap(&g, &p, &d, 0.25, None).unwrap();
    let second = apply_dmap(&first.geometry, &p, &d, 0.25, None).unwrap();
    let combined = apply_dmap(&g, &p, &d, 0.5, None).unwrap();
    assert_eq!(
        second.geometry.vertices[0].position,
        combined.geometry.vertices[0].position
    );
    assert_eq!(
        apply_dmap(&g, &p, &d, -0.5, None)
            .unwrap()
            .geometry
            .vertices[0]
            .position,
        [1.5, 0.0, -0.5]
    );
    let mut absent = g.clone();
    absent.uv_stitches = None;
    assert!(apply_dmap(&absent, &p, &d, 0.0, None).is_ok());
    assert!(apply_dmap(&absent, &p, &d, 1.0, None).is_err());
}
fn rig() -> Rig {
    Rig {
        bones: vec![
            Bone {
                name: "root".into(),
                opposing_index: 0,
                parent_index: -1,
                hash: 1,
                flags: 0,
                position: [0.0; 3],
                orientation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
            Bone {
                name: "child".into(),
                opposing_index: 1,
                parent_index: 0,
                hash: 2,
                flags: 0,
                position: [1.0, 0.0, 0.0],
                orientation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
        ],
        chains: vec![],
    }
}
fn bond(offset: f32) -> Bond {
    Bond {
        version: 1,
        adjustments: vec![BoneAdjustment {
            slot_hash: 1,
            offset: [offset, 0.0, 0.0],
            scale: [0.0; 3],
            quaternion: [0.0, 0.0, 0.0, 1.0],
        }],
        raw: vec![],
        unparsed_suffix: vec![],
    }
}
#[test]
fn bond_rig_weights_descendants_missing_hash_and_source_copy() {
    let r = rig();
    let mut b = bond(1.0);
    b.adjustments[0].scale = [0.5, 0.0, 0.0];
    let p = apply_bond_rig(&r, &b, 0.5).unwrap();
    assert_eq!(p.rig.bones[0].position, [0.5, 0.0, 0.0]);
    assert_eq!(p.rig.bones[1].position, [1.25, 0.0, 0.0]);
    assert_eq!(
        apply_bond_rig(&r, &b, -0.5).unwrap().rig.bones[1].position,
        [0.75, 0.0, 0.0]
    );
    assert_eq!(r.bones[1].position, [1.0, 0.0, 0.0]);
    b.adjustments[0].slot_hash = 99;
    assert!(apply_bond_rig(&r, &b, 1.0).is_err());
    assert!(apply_bond_rig(&r, &b, 0.0).is_ok());
}
#[test]
fn bond_mesh_accumulates_wire_order_and_preserves_inputs() {
    let r = rig();
    let mut g = geom();
    g.vertices.truncate(1);
    g.vertices[0].has_skin = true;
    g.vertices[0].joints = [0; 4];
    g.vertices[0].raw_weights = [255, 0, 0, 0];
    g.bone_palette = vec![1];
    let mut b = bond(1.0);
    b.adjustments.push(bond(2.0).adjustments[0].clone());
    let result = apply_bond_mesh(&g, &r, &b, 1.0).unwrap();
    assert_eq!(result.geometry.vertices[0].position, [4.0, 0.0, 0.0]);
    assert_eq!(result.geometry.vertices[0].normal, [0.0, 1.0, 0.0]);
    assert_eq!(result.geometry.original, g.original);
    assert_eq!(g.vertices[0].position, [1.0, 0.0, 0.0]);
    g.vertices[0].joints[0] = 9;
    assert!(apply_bond_mesh(&g, &r, &b, 1.0).is_err());
    assert!(apply_bond_mesh(&g, &r, &b, 0.0).is_ok());
}
#[test]
fn bond_private_key_header_and_truncation() {
    let mut b = vec![];
    for v in [3, 0, 0, 0, 1] {
        put32(&mut b, v);
    }
    b.extend([0; 16]);
    put32(&mut b, 44);
    put32(&mut b, 52);
    put32(&mut b, 1);
    put32(&mut b, 1);
    put32(&mut b, 1);
    for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
        b.extend(v.to_le_bytes());
    }
    let bond = Bond::parse(&b).unwrap();
    assert_eq!(bond.adjustments.len(), 1);
    assert_eq!(bond.raw, b);
    for n in 0..b.len() {
        assert!(Bond::parse(&b[..n]).is_err());
    }
}
