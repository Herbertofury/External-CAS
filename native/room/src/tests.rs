use crate::*;
use external_cas_assets::ResourceKey;
use std::sync::atomic::AtomicBool;
fn chunk(tag: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}
fn rcol_single(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for n in [3u32, 1, 0, 0, 1] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.extend_from_slice(&1u64.to_le_bytes());
    out.extend_from_slice(&0x1234u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&4i32.to_le_bytes());
    out.extend_from_slice(body);
    out
}
#[test]
fn rcol_single_chunk_offset_rule_and_truncation() {
    let bytes = rcol_single(b"TESTpayload");
    let parsed = Rcol::parse(&bytes).unwrap();
    assert_eq!(&bytes[parsed.chunks[0].extent.clone()], b"TESTpayload");
    assert_eq!(parsed.chunks[0].declared_offset, 0);
    for cut in 0..48 {
        assert!(Rcol::parse(&bytes[..cut]).is_err());
    }
    let mut bad = bytes;
    bad[4..8].copy_from_slice(&2u32.to_le_bytes());
    assert!(Rcol::parse(&bad).is_err());
}
#[test]
fn public_private_and_external_one_based_references_are_distinct() {
    let r = Rcol {
        version: 3,
        public_count: 1,
        unused: 0,
        chunks: vec![
            RcolChunk {
                key: ResourceKey::new(1, 0, 1),
                extent: 0..4,
                declared_offset: 0,
                declared_size: 4,
                tag: *b"TEST",
            },
            RcolChunk {
                key: ResourceKey::new(1, 0, 2),
                extent: 4..8,
                declared_offset: 4,
                declared_size: 4,
                tag: *b"TEST",
            },
        ],
        externals: vec![ResourceKey::new(2, 0, 3)],
    };
    assert!(matches!(r.reference(0), ChunkReference::Null));
    assert!(matches!(
        r.reference(1),
        ChunkReference::Internal { index: 0, .. }
    ));
    assert!(matches!(
        r.reference(0x10000001),
        ChunkReference::Internal { index: 1, .. }
    ));
    assert!(matches!(
        r.reference(0x30000001),
        ChunkReference::External { index: 0, .. }
    ));
    assert!(matches!(r.reference(2), ChunkReference::Unsupported { .. }));
    assert!(matches!(
        r.reference(0x10000000),
        ChunkReference::Unsupported { .. }
    ));
}
#[test]
fn world_unknown_bytes_and_nonfinite_bits_are_preserved() {
    let mut fields = chunk(*b"DGUD", &123u64.to_le_bytes());
    fields.extend(chunk(
        *b"POS ",
        &[0x80000000u32, 0x7fc12345, 0x3f800000]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    ));
    fields.extend(chunk([255, b'A', b'B', b'C'], &[0, 9, 255]));
    let source = chunk(*b"OMGS", &chunk(*b"OMGR", &chunk(*b"OBJ ", &fields)));
    let parsed = parse_world_data(&source).unwrap();
    assert_eq!(
        parsed.objects[0].position_bits,
        Some([0x80000000, 0x7fc12345, 0x3f800000])
    );
    assert_eq!(
        parsed.objects[0].opaque_fields[0],
        ([255, b'A', b'B', b'C'], vec![0, 9, 255])
    );
    for cut in 0..source.len() {
        assert!(parse_world_data(&source[..cut]).is_err());
    }
    let mut fields = chunk(*b"DGUD", &1u64.to_le_bytes());
    fields.extend(chunk(*b"DGUD", &2u64.to_le_bytes()));
    assert!(
        parse_world_data(&chunk(
            *b"OMGS",
            &chunk(*b"OMGR", &chunk(*b"OBJ ", &fields))
        ))
        .is_err()
    );
}
#[test]
fn material_invalid_float_bits_remain_exact_and_unapplied() {
    let mut body = Vec::new();
    body.extend_from_slice(b"MATD");
    for value in [0x103u32, 0, SHADER_FULL_BRIGHT, 48, 0, 0] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body.extend_from_slice(b"MTRL");
    for value in [
        0u32, 0, 1, POS_SCALE, 1, 4, 32, 0x3f800000, 0x7fc12345, 0x80000000, 0x7f800000,
    ] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    let c = RcolChunk {
        key: ResourceKey::default(),
        extent: 0..body.len(),
        declared_offset: 0,
        declared_size: body.len() as i32,
        tag: *b"MATD",
    };
    let parsed = parse_material(&body, &c).unwrap();
    assert_eq!(
        parsed.fields[0].float_bits,
        Some(vec![0x3f800000, 0x7fc12345, 0x80000000, 0x7f800000])
    );
    assert!(parsed.float_values(POS_SCALE).is_err());
    for cut in 0..body.len() {
        let mut c = c.clone();
        c.extent.end = cut;
        assert!(parse_material(&body[..cut], &c).is_err());
    }
}
fn mesh_fixture() -> (Vec<u8>, MeshRecord, VertexFormat, RcolChunk, RcolChunk) {
    let mut bytes = b"VBUF".to_vec();
    for value in [1u32, 0, 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for x in [0i16, 780, 1560] {
        for value in [x, 0, 0, 780] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[127, 255, 127, 0]);
        bytes.extend_from_slice(&[0; 4]);
    }
    let end = bytes.len();
    bytes.extend_from_slice(b"IBUF");
    for value in [1u32, 1, 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0i16, 1, 1] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    let vb = RcolChunk {
        key: ResourceKey::default(),
        extent: 0..end,
        declared_offset: 0,
        declared_size: end as i32,
        tag: *b"VBUF",
    };
    let ib = RcolChunk {
        key: ResourceKey::default(),
        extent: end..bytes.len(),
        declared_offset: end as u32,
        declared_size: (bytes.len() - end) as i32,
        tag: *b"IBUF",
    };
    let f = VertexFormat {
        version: 2,
        stride: 16,
        extended: 0,
        layouts: vec![
            VertexLayout {
                usage: 0,
                usage_index: 0,
                format: 7,
                offset: 0,
                bytes: Some(8),
            },
            VertexLayout {
                usage: 1,
                usage_index: 0,
                format: 8,
                offset: 8,
                bytes: Some(4),
            },
            VertexLayout {
                usage: 2,
                usage_index: 0,
                format: 6,
                offset: 12,
                bytes: Some(4),
            },
        ],
    };
    let m = MeshRecord {
        extent: 0..0,
        name: 0,
        material: 0,
        vertex_format: 0,
        vertex_buffer: 0,
        index_buffer: 0,
        primitive_type: 3,
        flags: 0,
        stream_offset: 0,
        start_vertex: 0,
        start_index: 0,
        minimum_vertex: 0,
        vertex_count: 3,
        primitive_count: 1,
        parameter_material: None,
        joint_count: None,
        skin_controller: None,
        joint_references: Vec::new(),
    };
    (bytes, m, f, vb, ib)
}
#[test]
fn differenced_indices_local_vertex_ranges_budgets_and_cancel() {
    let (bytes, m, f, vb, ib) = mesh_fixture();
    let decoded = extract_mesh(&bytes, &m, f.clone(), &vb, &ib, 3, 1).unwrap();
    assert_eq!(decoded.indices, [0, 1, 2]);
    assert_eq!(
        decoded.source_tool_positions,
        [[0., 0., 0.], [1., 0., 0.], [2., 0., 0.]]
    );
    assert!(extract_mesh(&bytes, &m, f.clone(), &vb, &ib, 2, 1).is_err());
    assert!(
        extract_mesh_cancellable(
            &bytes,
            &m,
            f.clone(),
            &vb,
            &ib,
            3,
            1,
            &AtomicBool::new(true)
        )
        .is_err()
    );
    let mut bad = bytes.clone();
    let n = bad.len();
    bad[n - 2..].copy_from_slice(&100i16.to_le_bytes());
    assert!(extract_mesh(&bad, &m, f.clone(), &vb, &ib, 3, 1).is_err());
    let mut flags = bytes;
    flags[ib.extent.start + 8..ib.extent.start + 12].copy_from_slice(&4u32.to_le_bytes());
    assert!(extract_mesh(&flags, &m, f, &vb, &ib, 3, 1).is_err());
}

#[test]
fn skin_binding_bytes_do_not_authorize_an_invented_runtime_palette() {
    let (bytes, mut record, format, vb, ib) = mesh_fixture();
    let mut geometry = extract_mesh(&bytes, &record, format, &vb, &ib, 3, 1).unwrap();
    for v in &mut geometry.source_vertices {
        v.blend_indices = Some([0; 4]);
        v.blend_weights_ubyte4n = Some([255, 0, 0, 0]);
    }
    geometry.skinning_status = RoomSkinningStatus::UnqualifiedBindings;
    record.joint_references = vec![0xcd68f001];
    let mut skin = RoomSkin {
        version: 1,
        bones: vec![RoomSkinBone {
            hash: 0xcd68f001,
            inverse_bind_bits: [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.].map(f32::to_bits),
        }],
    };
    let error = qualify_static_skinning(&mut geometry, &record, Some(&skin), true).unwrap_err();
    assert!(error.contains("does not establish actual shader palette"));
    assert!(matches!(
        geometry.skinning_status,
        RoomSkinningStatus::UnqualifiedBindings
    ));
    skin.bones[0].inverse_bind_bits[3] = 1f32.to_bits();
    assert!(
        qualify_static_skinning(&mut geometry, &record, Some(&skin), true)
            .unwrap_err()
            .contains("not identity")
    );
    assert!(
        qualify_static_skinning(&mut geometry, &record, Some(&skin), false)
            .unwrap_err()
            .contains("observed object rig")
    );
}
#[test]
fn skin_framing_preserves_nonfinite_inverse_bind_and_rejects_truncation() {
    let mut raw = b"SKIN".to_vec();
    for v in [1u32, 1, 0xcd68f001] {
        raw.extend_from_slice(&v.to_le_bytes());
    }
    for i in 0..12 {
        raw.extend_from_slice(&(if i == 3 { 0x7fc12345u32 } else { 0 }).to_le_bytes());
    }
    let chunk = RcolChunk {
        key: Default::default(),
        extent: 0..raw.len(),
        declared_offset: 0,
        declared_size: raw.len() as i32,
        tag: *b"SKIN",
    };
    let parsed = parse_room_skin(&raw, &chunk).unwrap();
    assert_eq!(parsed.bones[0].inverse_bind_bits[3], 0x7fc12345);
    for end in 0..raw.len() {
        assert!(
            parse_room_skin(
                &raw[..end],
                &RcolChunk {
                    extent: 0..end,
                    ..chunk.clone()
                }
            )
            .is_err()
        );
    }
}
#[test]
fn source_world_matrix_requires_explicit_origin_and_unit_quaternion() {
    let q = (0.5f32).sqrt();
    let object = WorldObject {
        object_id: None,
        definition_instance: None,
        model_key: None,
        position_bits: Some([128., 150., 128.].map(f32::to_bits)),
        rotation_bits: Some([0., q, 0., q].map(f32::to_bits)),
        scale_bits: Some(2f32.to_bits()),
        opaque_fields: Vec::new(),
    };
    let m = object.matrix_relative_to([128., 149., 128.]).unwrap();
    assert!(m[0].abs() < 0.000001);
    assert!((m[2] + 2.).abs() < 0.000001);
    assert_eq!(m[13], 1.);
    let mut invalid = object;
    invalid.rotation_bits = Some([0.; 4].map(f32::to_bits));
    assert!(invalid.matrix_relative_to([0.; 3]).is_err());
}
fn dds(width: u32, height: u32, kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![0; 128];
    out[..4].copy_from_slice(b"DDS ");
    for (at, value) in [
        (4, 124u32),
        (8, 0x1007),
        (12, height),
        (16, width),
        (76, 32),
        (80, 4),
        (108, 0x1000),
    ] {
        out[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    out[84..88].copy_from_slice(kind);
    out.extend_from_slice(body);
    out
}
#[test]
fn dst1_streams_decode_two_distinct_blocks_without_source_mutation() {
    let source = dds(
        8,
        4,
        b"DST1",
        &[0, 248, 0, 0, 224, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let original = source.clone();
    let (w, h, pixels, status) = decode_room_texture(&source).unwrap();
    assert_eq!((w, h), (8, 4));
    assert!(status.contains("DST"));
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
    assert_eq!(&pixels[4 * 4..5 * 4], &[0, 255, 0, 255]);
    assert_eq!(source, original);
    let mut bad = source;
    bad.pop();
    assert!(decode_room_texture(&bad).is_err());
}
#[test]
fn bgra_pitch_and_rgba_preserve_actual_alpha() {
    let mut source = dds(2, 1, &[0; 4], &[3, 2, 1, 0, 30, 20, 10, 128]);
    for (at, value) in [
        (8, 0x100fu32),
        (20, 8),
        (80, 0x41),
        (88, 32),
        (92, 0x00ff0000),
        (96, 0x0000ff00),
        (100, 0x000000ff),
        (104, 0xff000000),
    ] {
        source[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    assert_eq!(
        decode_room_texture(&source).unwrap().2,
        [1, 2, 3, 0, 10, 20, 30, 128]
    );
    source.pop();
    assert!(decode_room_texture(&source).is_err());
}
