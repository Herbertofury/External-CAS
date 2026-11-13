// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{
    dbpf::{CASP, GEOM, decode_refpack},
    scan::Catalog,
    *,
};
use std::{io::Write, sync::atomic::AtomicBool};
fn u32b(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
fn u16b(out: &mut Vec<u8>, n: u16) {
    out.extend(n.to_le_bytes());
}
fn f32b(out: &mut Vec<u8>, n: f32) {
    out.extend(n.to_le_bytes());
}
fn package(flags: u32, compression: u16, payload: &[u8], expanded: u32) -> Vec<u8> {
    let mut b = vec![0; 96];
    b[..4].copy_from_slice(b"DBPF");
    b[4..8].copy_from_slice(&2u32.to_le_bytes());
    b[8..12].copy_from_slice(&1u32.to_le_bytes());
    b[36..40].copy_from_slice(&1u32.to_le_bytes());
    b.extend(payload);
    let at = b.len() as u32;
    let mut index = Vec::new();
    u32b(&mut index, flags);
    for (i, x) in [CASP, 0x12345678, 0xfedcba98].into_iter().enumerate() {
        if flags & (1 << i) != 0 {
            u32b(&mut index, x);
        }
    }
    for (i, x) in [CASP, 0x12345678, 0xfedcba98].into_iter().enumerate() {
        if flags & (1 << i) == 0 {
            u32b(&mut index, x);
        }
    }
    for x in [0x76543210, 96, 0x80000000 | payload.len() as u32, expanded] {
        u32b(&mut index, x);
    }
    u16b(&mut index, compression);
    u16b(&mut index, 1);
    b[44..48].copy_from_slice(&(index.len() as u32).to_le_bytes());
    b[64..68].copy_from_slice(&at.to_le_bytes());
    b.extend(index);
    b
}
#[test]
fn all_shared_flags_high_tgi_raw_zlib_and_refpack() {
    let original = b"actual payload bytes";
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(original).unwrap();
    let z = z.finish().unwrap();
    for flags in 0..8 {
        for (compression, payload) in [(0, original.as_slice()), (0x5a42, z.as_slice())] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("data.package");
            std::fs::write(
                &path,
                package(flags, compression, payload, original.len() as u32),
            )
            .unwrap();
            let idx = PackageIndex::open(&path).unwrap();
            assert_eq!(
                idx.entries[0].key,
                ResourceKey::new(CASP, 0x12345678, 0xfedcba9876543210)
            );
            assert_eq!(
                idx.read_resource(&idx.entries[0], ReadLimits::default())
                    .unwrap(),
                original
            );
        }
    }
    let rp = [
        0x10, 0xfb, 0, 0, 7, 0xe0, b'a', b'b', b'c', b'd', 0xff, b'e', b'f', b'g',
    ];
    assert_eq!(
        decode_refpack(&rp, 7, ReadLimits::default(), &AtomicBool::new(false)).unwrap(),
        b"abcdefg"
    );
    // Short command with an overlapping backreference.
    let rp = [
        0x10, 0xfb, 0, 0, 8, 0xe0, b'a', b'b', b'c', b'd', 0x04, 0x03, 0xfc,
    ];
    assert_eq!(
        decode_refpack(&rp, 8, ReadLimits::default(), &AtomicBool::new(false)).unwrap(),
        b"abcdabcd"
    );
}
#[test]
#[ignore = "requires explicit private S4PY_PACKAGE_QUALIFICATION JSON; source is read-only"]
fn installed_package_payloads_match_independent_s4py() {
    use sha2::{Digest, Sha256};
    let manifest = std::env::var_os("S4PY_PACKAGE_QUALIFICATION").expect("private manifest");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let index = PackageIndex::open(manifest["path"].as_str().unwrap()).unwrap();
    let mut reader = index.reader().unwrap();
    let rows = manifest["rows"].as_array().unwrap();
    assert_eq!(rows.len(), index.entries.len());
    for (entry, expected) in index.entries.iter().zip(rows) {
        assert_eq!(entry.key.to_string(), expected["key"].as_str().unwrap());
        let bytes = reader.read_resource(entry, ReadLimits::default()).unwrap();
        assert_eq!(bytes.len() as u64, expected["expanded"].as_u64().unwrap());
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected["sha256"].as_str().unwrap());
        assert_eq!(entry.compression as u64, expected["compression"][0].as_u64().unwrap());
        assert_eq!(entry.committed as u64, expected["compression"][1].as_u64().unwrap());
    }
    index.revalidate(&AtomicBool::new(false)).unwrap();
}

#[test]
fn zero_unknown2_preserves_codec_size_and_pin_guards() {
    // Real packages use Unknown2=0 with zlib. It is not a committed flag.
    let original = b"abcdefg";
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(original).unwrap();
    let z = z.finish().unwrap();
    let rp = [0x10, 0xfb, 0, 0, 7, 0xe0, b'a', b'b', b'c', b'd', 0xff, b'e', b'f', b'g'];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unknown2.package");
    for (compression, payload) in [(0, original.as_slice()), (0x5a42, z.as_slice()), (0xffff, rp.as_slice())] {
        let mut bytes = package(0, compression, payload, original.len() as u32);
        let end = bytes.len();
        bytes[end - 2..].copy_from_slice(&0u16.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let idx = PackageIndex::open(&path).unwrap();
        assert_eq!(idx.entries[0].committed, 0);
        assert_eq!(idx.read_resource(&idx.entries[0], ReadLimits::default()).unwrap(), original);
        let wrong_pin = [0; 32];
        assert!(matches!(idx.reader().unwrap().read_pinned_resource(&idx.entries[0], wrong_pin, ReadLimits::default(), &AtomicBool::new(false)), Err(Error::Changed)));
        // Metadata zero does not relax declared expanded-size validation.
        let index = u32::from_le_bytes(bytes[64..68].try_into().unwrap()) as usize;
        let expanded_at = index + 4 + 6 * 4;
        bytes[expanded_at..expanded_at + 4].copy_from_slice(&8u32.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let idx = PackageIndex::open(&path).unwrap();
        assert!(idx.read_resource(&idx.entries[0], ReadLimits::default()).is_err());
        bytes[end - 2..].copy_from_slice(&2u16.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let idx = PackageIndex::open(&path).unwrap();
        assert!(matches!(idx.read_resource(&idx.entries[0], ReadLimits::default()), Err(Error::Unsupported(_))));
    }
}

#[test]
fn malformed_resources_stale_sources_and_budget_are_not_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.package");
    let b = package(0, 0, b"ok", 2);
    std::fs::write(&path, &b).unwrap();
    let idx = PackageIndex::open(&path).unwrap();
    let mut fake = idx.entries[0].clone();
    fake.offset += 1;
    assert!(idx.read_resource(&fake, ReadLimits::default()).is_err());
    assert!(matches!(
        idx.read_resource(
            &idx.entries[0],
            ReadLimits {
                max_resource_bytes: 1
            }
        ),
        Err(Error::Budget { .. })
    ));
    let mut changed = b.clone();
    changed[96] = b'X';
    std::fs::write(&path, &changed).unwrap();
    // This case proves the metadata-change contract. Immediate same-size writes
    // can retain the same observable timestamp on Windows filesystems; make the
    // metadata difference explicit rather than depending on clock granularity.
    // The retained_payload_pin test separately proves same-stamp rewrites fail.
    let changed_time =
        std::fs::metadata(&path).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(changed_time))
        .unwrap();
    assert!(matches!(
        idx.read_resource(&idx.entries[0], ReadLimits::default()),
        Err(Error::Changed)
    ));
    for n in 0..b.len() {
        std::fs::write(&path, &b[..n]).unwrap();
        assert!(PackageIndex::open(&path).is_err(), "truncated package {n}");
    }
    for bad in [
        &[0x10, 0xfb, 0, 0, 4, 0x04, 0, 0xfc][..],
        &[0x10, 0xfb, 0, 0, 1, 0xfd][..],
        &[0x10, 0xfb, 0, 0, 0, 0xfc, 0][..],
    ] {
        assert!(decode_refpack(bad, 4, ReadLimits::default(), &AtomicBool::new(false)).is_err());
    }
    std::fs::write(&path, package(0, 0x5a42, b"bad zlib", 100)).unwrap();
    let idx = PackageIndex::open(&path).unwrap();
    assert!(
        idx.read_resource(&idx.entries[0], ReadLimits::default())
            .is_err()
    );
}
#[test]
fn scan_retains_duplicate_providers_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["one.package", "two.package"] {
        std::fs::write(dir.path().join(name), package(7, 0, b"ok", 2)).unwrap();
    }
    let catalog =
        Catalog::scan_roots(&[dir.path().to_path_buf()], &AtomicBool::new(false), |_| {}).unwrap();
    let key = catalog.packages[0].entries[0].key;
    assert_eq!(catalog.providers(key).len(), 2);
    assert!(catalog.unique(key).is_err());
    assert_eq!(catalog.progress.resources, 2);
    assert_eq!(catalog.casp_candidates(key.instance_id).len(), 1);
    assert!(catalog.unique_casp_instance(key.instance_id).is_err());
    let mut different_group = package(7, 0, b"ok", 2);
    let at = u32::from_le_bytes(different_group[64..68].try_into().unwrap()) as usize;
    different_group[at + 8..at + 12].copy_from_slice(&42u32.to_le_bytes());
    std::fs::write(dir.path().join("third.package"), different_group).unwrap();
    let broader =
        Catalog::scan_roots(&[dir.path().to_path_buf()], &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(broader.casp_candidates(key.instance_id).len(), 2);
    assert!(broader.unique_casp_instance(key.instance_id).is_err());
    assert!(matches!(
        Catalog::scan_roots(&[dir.path().to_path_buf()], &AtomicBool::new(true), |_| {}),
        Err(Error::Cancelled)
    ));
}

#[test]
fn texture_family_candidates_never_choose_a_group_or_source_winner() {
    let dir = tempfile::tempdir().unwrap();
    for (name, ty) in [
        ("rle.package", 0x3453cf95u32),
        ("lrle.package", 0x2bc04edf),
        ("lrle-copy.package", 0x2bc04edf),
    ] {
        let mut b = package(7, 0, b"ok", 2);
        let at = u32::from_le_bytes(b[64..68].try_into().unwrap()) as usize;
        b[at + 4..at + 8].copy_from_slice(&ty.to_le_bytes());
        std::fs::write(dir.path().join(name), b).unwrap();
    }
    let catalog =
        Catalog::scan_roots(&[dir.path().to_path_buf()], &AtomicBool::new(false), |_| {}).unwrap();
    let key = ResourceKey::new(0x3453cf95, 0x12345678, 0xfedcba9876543210);
    let candidates = catalog.texture_candidates(key, false);
    assert_eq!(
        candidates
            .iter()
            .find(|c| c.kind == "exact")
            .unwrap()
            .providers
            .len(),
        1
    );
    assert_eq!(
        candidates
            .iter()
            .find(|c| c.key.type_id == 0x2bc04edf)
            .unwrap()
            .providers
            .len(),
        2
    );
    assert!(
        candidates
            .iter()
            .all(|c| c.key.group_id == key.group_id && c.key.instance_id == key.instance_id)
    );
    assert_eq!(
        catalog
            .texture_candidates(ResourceKey::new(GEOM, key.group_id, key.instance_id), true)
            .len(),
        1
    );
}
#[test]
fn retained_payload_pin_detects_same_timestamp_same_index_rewrite() {
    use sha2::{Digest, Sha256};
    use std::io::{Seek, SeekFrom};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinned.package");
    std::fs::write(&path, package(0, 0, b"ok", 2)).unwrap();
    let index = PackageIndex::open(&path).unwrap();
    let entry = &index.entries[0];
    let stamp = std::fs::metadata(&path).unwrap().modified().unwrap();
    let original = index.read_resource(entry, ReadLimits::default()).unwrap();
    let pin: [u8; 32] = Sha256::digest(&original).into();
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(96)).unwrap();
    file.write_all(b"NO").unwrap();
    file.flush().unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(stamp))
        .unwrap();
    drop(file);
    assert_eq!(
        index.read_resource(entry, ReadLimits::default()).unwrap(),
        b"NO"
    );
    assert!(matches!(
        index.reader().unwrap().read_pinned_resource(
            entry,
            pin,
            ReadLimits::default(),
            &AtomicBool::new(false)
        ),
        Err(Error::Changed)
    ));
}

fn casp(version: u32) -> Vec<u8> {
    casp_with_parent(version, None)
}
fn casp_with_parent(version: u32, parent: Option<ResourceKey>) -> Vec<u8> {
    casp_with_slider_block(version, parent, None)
}
fn casp_with_slider_block(
    version: u32,
    parent: Option<ResourceKey>,
    slider_block: Option<[u8; 44]>,
) -> Vec<u8> {
    casp_with_retained_fields(version,parent,slider_block,0)
}
fn casp_with_retained_fields(
    version:u32,parent:Option<ResourceKey>,slider_block:Option<[u8;44]>,shared_uv_space:u32,
) -> Vec<u8> {
    casp_with_region_overrides(version,parent,slider_block,shared_uv_space,&[])
}
fn casp_with_region_overrides(
    version:u32,parent:Option<ResourceKey>,slider_block:Option<[u8;44]>,shared_uv_space:u32,
    overrides:&[(u8,u32)],
) -> Vec<u8> {
    let mut b = Vec::new();
    for n in [version, 0, 0] {
        u32b(&mut b, n);
    }
    let name = "Real catalog name"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect::<Vec<_>>();
    b.push(name.len() as u8);
    b.extend(name);
    f32b(&mut b, 1.);
    u16b(&mut b, 2);
    u32b(&mut b, 3);
    u32b(&mut b, 4);
    b.push(0x85);
    if version >= 39 {
        b.push(7);
    }
    if version >= 50 {
        u16b(&mut b, 7);
    }
    if version >= 51 {
        u32b(&mut b, 3);
        for word in [1u64, u64::MAX, 1u64 << 63] {
            b.extend(word.to_le_bytes());
        }
    } else {
        b.extend(0xfedcba9876543210u64.to_le_bytes());
        if version >= 41 {
            b.extend(0x8000000000000001u64.to_le_bytes());
        }
    }
    if version > 36 {
        b.extend(0x80000000fedcba98u64.to_le_bytes());
    } else {
        u32b(&mut b, 0xfedcba98);
    }
    u32b(&mut b, 0);
    for n in [1, 2, 3] {
        u32b(&mut b, n);
    }
    if version > 42 {
        u32b(&mut b, 4);
    }
    b.push(9);
    u32b(&mut b, 5);
    u32b(&mut b, 11);
    b.extend([4, 0x20]);
    u16b(&mut b, 0);
    if version >= 32 {
        u32b(&mut b, 1);
    }
    if version >= 34 {
        u16b(&mut b, 10);
        b.push(0);
        b.extend([0; 9]);
    } else {
        b.push(0);
    }
    b.extend([0, 0, 0]);
    if version >= 28 {
        b.extend([0; 8]);
    }
    if version >= 30 {
        b.push(0);
    }
    if version >= 31 {
        u32b(&mut b, 0);
    }
    if version >= 46 {
        b.extend([0; 8]);
    }
    if version >= 38 {
        b.extend(0xfedcba9876543210u64.to_le_bytes());
    }
    if version >= 39 {
        b.extend(0x8000000000000001u64.to_le_bytes());
    }
    if version >= 44 {
        b.extend(slider_block.unwrap_or([0; 44]));
    }
    if version >= 46 {
        b.push(0);
    }
    b.extend([0, if parent.is_some() { 3 } else { 0 }]);
    u32b(&mut b, 0);
    b.extend([1, 0]);
    u32b(&mut b, 0);
    b.extend([0, 1, 1, 0, 2, 0, 0, 0]);
    b.push(overrides.len().try_into().unwrap());
    for (region,bits) in overrides { b.push(*region);u32b(&mut b,*bits); }
    b.extend([0,0]);
    if version >= 27 {
        u32b(&mut b, shared_uv_space);
    }
    if version >= 29 {
        b.push(0);
    }
    if version >= 42 {
        b.push(0);
    }
    if version >= 49 {
        b.push(0);
    }
    if version >= 52 {
        b.push(0);
    }
    let offset = b.len() as u32 - 8;
    b[4..8].copy_from_slice(&offset.to_le_bytes());
    b.push(if parent.is_some() { 4 } else { 3 });
    for (ty, gr, inst) in [(0, 0, 0u64), (GEOM, 0x22, 0x11), (0x2f7d0004, 0x44, 0x33)] {
        b.extend(inst.to_le_bytes());
        u32b(&mut b, gr);
        u32b(&mut b, ty);
    }
    if let Some(key) = parent {
        b.extend(key.instance_id.to_le_bytes());
        u32b(&mut b, key.group_id);
        u32b(&mut b, key.type_id);
    }
    b
}
#[test]
fn casp_versions_preserve_labels_tgi_roles_and_reject_unsupported_or_truncated() {
    for version in 26..=52 {
        let b = casp(version);
        let c = CasPart::parse(&b).unwrap_or_else(|e| panic!("v{version} {e}"));
        assert_eq!(c.name, "Real catalog name");
        assert_eq!(c.body_type, 5);
        assert_eq!(c.lod0_meshes, [ResourceKey::new(GEOM, 0x22, 0x11)]);
        assert_eq!(c.diffuse, Some(ResourceKey::new(0x2f7d0004, 0x44, 0x33)));
        assert_eq!(c.color_sliders.is_some(), version >= 44);
        assert_eq!(c.shared_uv_space, if version>=27 {Some(0)}else{None});
        assert_eq!(c.effective_shared_uv_body_type(),Some(c.body_type));
        assert_eq!(c.region_overrides.as_ref().unwrap().len(),0);
        assert!(c.disallow_opposite_gender());
        assert_eq!(
            c.disallow_opposite_frame(),
            if version >= 39 { Some(true) } else { None }
        );
        assert_eq!(c.has_mesh(), Some(true));
        let compatibility = c.compatibility.as_ref().unwrap();
        assert_eq!(compatibility.lod_count, 1);
        assert_eq!(
            compatibility.exclude_part_flags,
            if version >= 51 {
                vec![1, u64::MAX, 1 << 63]
            } else if version >= 41 {
                vec![0xfedcba9876543210, 0x8000000000000001]
            } else {
                vec![0xfedcba9876543210]
            }
        );
        assert_eq!(
            compatibility.exclude_modifier_region_flags,
            if version > 36 {
                0x80000000fedcba98
            } else {
                0xfedcba98
            }
        );
        assert_eq!(
            compatibility.opposite_gender_part,
            if version >= 38 {
                Some(0xfedcba9876543210)
            } else {
                None
            }
        );
        assert_eq!(
            compatibility.fallback_part,
            if version >= 39 {
                Some(0x8000000000000001)
            } else {
                None
            }
        );
        for n in 0..b.len() {
            assert!(CasPart::parse(&b[..n]).is_err());
        }
    }
    let mut b = casp(52);
    b[..4].copy_from_slice(&99u32.to_le_bytes());
    assert!(matches!(CasPart::parse(&b), Err(Error::Unsupported(_))));
}
#[test]
fn shared_uv_raw_zero_override_unknown_and_version_boundary_are_distinct() {
    for version in [26,27,44,52] {
        for raw in [0,2,5,0x80000000,u32::MAX] {
            let bytes=casp_with_retained_fields(version,None,None,raw);
            let part=CasPart::parse(&bytes).unwrap();
            assert_eq!(part.shared_uv_space,if version>=27 {Some(raw)}else{None});
            assert_eq!(part.effective_shared_uv_body_type(),Some(if version<27||raw==0{5}else{raw}));
            let mut encoded=serde_json::to_value(&part).unwrap();
            let restored:CasPart=serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(restored.shared_uv_space,part.shared_uv_space);
            encoded.as_object_mut().unwrap().remove("shared_uv_space");
            let older:CasPart=serde_json::from_value(encoded).unwrap();
            assert_eq!(older.effective_shared_uv_body_type(),if version<27{Some(5)}else{None});
        }
    }
}

#[test]
fn region_overrides_preserve_order_duplicates_raw_float_bits_and_unknown_old_cache() {
    let expected=[(7,0x80000000),(7,0x7fc01234),(255,0x7f800000),(0,0xbf800000)];
    for version in 26..=52 {
        let bytes=casp_with_region_overrides(version,None,None,0,&expected);
        let part=CasPart::parse(&bytes).unwrap();
        let actual=part.region_overrides.as_ref().unwrap().iter().map(|r|(r.region,r.layer_bits)).collect::<Vec<_>>();
        assert_eq!(actual,expected,"version{version}");
        let json=serde_json::to_value(&part).unwrap();
        let restored:CasPart=serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored.region_overrides.unwrap().iter().map(|r|(r.region,r.layer_bits)).collect::<Vec<_>>(),expected);
        let mut older=json;older.as_object_mut().unwrap().remove("region_overrides");
        let unknown:CasPart=serde_json::from_value(older).unwrap();assert!(unknown.region_overrides.is_none());
        assert_eq!(part.diffuse,Some(ResourceKey::new(0x2f7d0004,0x44,0x33)));
    }
}
#[test]
fn exact_casp_color_slider_bits_survive_invalid_values_cache_serde_and_version_boundaries() {
    use external_cas_assets::casp::{CaspColorChannel, CaspColorSliders};
    let bits = [
        0.2f32.to_bits(),
        0.05f32.to_bits(),
        (-0.5f32).to_bits(),
        0.5f32.to_bits(),
        0.05f32.to_bits(),
        (-0.25f32).to_bits(),
        0.75f32.to_bits(),
        0.02f32.to_bits(),
        (-0.75f32).to_bits(),
        0.25f32.to_bits(),
        0.01f32.to_bits(),
    ];
    let mut raw = [0; 44];
    for (out, bits) in raw.chunks_exact_mut(4).zip(bits) {
        out.copy_from_slice(&bits.to_le_bytes());
    }
    let expected = CaspColorSliders::from_raw_bytes(raw);
    assert_eq!(expected.raw_bytes(), raw);
    assert!(expected.validation_issues().is_empty());
    let hosv = expected.ranges_hosv();
    assert_eq!(hosv[0].minimum, -0.5);
    assert_eq!(hosv[1].maximum, 1.0);
    assert_eq!(hosv[2].maximum, 0.75);
    assert_eq!(hosv[3].minimum, -0.75);
    for version in 44..=52 {
        let parsed = CasPart::parse(&casp_with_slider_block(version, None, Some(raw))).unwrap();
        assert_eq!(parsed.color_sliders, Some(expected));
        assert_eq!(
            parsed.diffuse,
            Some(ResourceKey::new(0x2f7d0004, 0x44, 0x33))
        );
        let encoded = serde_json::to_vec(&parsed).unwrap();
        let roundtrip: CasPart = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(roundtrip.color_sliders, Some(expected));
    }
    for (index, invalid) in [
        (0, (-1f32).to_bits()),
        (2, 0x7fc12345),
        (4, (-0f32).to_bits()),
        (6, (-2f32).to_bits()),
        (10, f32::INFINITY.to_bits()),
    ] {
        let mut invalid_raw = raw;
        invalid_raw[index * 4..index * 4 + 4].copy_from_slice(&invalid.to_le_bytes());
        let parsed = CasPart::parse(&casp_with_slider_block(52, None, Some(invalid_raw))).unwrap();
        let sliders = parsed.color_sliders.unwrap();
        assert_eq!(sliders.raw_bytes(), invalid_raw);
        assert!(!sliders.validation_issues().is_empty());
        let cached: CasPart =
            serde_json::from_slice(&serde_json::to_vec(&parsed).unwrap()).unwrap();
        assert_eq!(cached.color_sliders.unwrap().raw_bytes(), invalid_raw);
    }
    let mut invalid = expected;
    invalid.opacity.minimum.bits = (-0.1f32).to_bits();
    assert_eq!(
        invalid.validation_issues()[0].channel,
        CaspColorChannel::Opacity
    );
    assert!(
        CasPart::parse(&casp_with_slider_block(43, None, Some(raw)))
            .unwrap()
            .color_sliders
            .is_none()
    );
}
fn geom() -> Vec<u8> {
    let mut c = b"GEOM".to_vec();
    for n in [12, 0, 4, 0, 0, 0, 3, 5] {
        u32b(&mut c, n);
    }
    for (usage, subtype, size) in [(1, 1, 12), (2, 1, 12), (3, 1, 8), (4, 2, 4), (5, 2, 4)] {
        u32b(&mut c, usage);
        u32b(&mut c, subtype);
        c.push(size);
    }
    for pos in [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
        for x in pos.into_iter().chain([0., 0., 1., 0., 0.]) {
            f32b(&mut c, x);
        }
        c.extend([0; 4]);
        c.extend([255, 0, 0, 0]);
    }
    u32b(&mut c, 1);
    c.push(2);
    u32b(&mut c, 3);
    for n in [0, 1, 2] {
        u16b(&mut c, n);
    }
    for n in [0, 0, 1, 0x12345678, 0] {
        u32b(&mut c, n);
    }
    let offset = c.len() as u32 - 16;
    c[8..12].copy_from_slice(&offset.to_le_bytes());
    let mut b = Vec::new();
    for n in [3, 0, 0, 0, 1] {
        u32b(&mut b, n);
    }
    b.extend([0; 16]);
    u32b(&mut b, 44);
    u32b(&mut b, c.len() as u32);
    b.extend(c);
    b
}
#[test]
fn geom_extracts_skin_positions_uv_and_triangle_and_rejects_malformed() {
    let bytes = geom();
    let g = Geometry::parse(&bytes).unwrap();
    assert_eq!(g.version, 12);
    assert_eq!(g.vertices.len(), 3);
    assert_eq!(g.indices, [0, 1, 2]);
    assert_eq!(g.bone_palette, [0x12345678]);
    assert_eq!(g.vertices[1].position, [1., 0., 0.]);
    assert_eq!(g.vertices[0].weights, [1., 0., 0., 0.]);
    assert_eq!(g.vertices[0].raw_weights, [255, 0, 0, 0]);
    for n in 0..bytes.len() {
        assert!(Geometry::parse(&bytes[..n]).is_err(), "truncated GEOM {n}");
    }
    let mut bad = bytes.clone();
    bad[44 + 36 + 45..44 + 36 + 49].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(Geometry::parse(&bad).is_err());
}

fn rig_fixture(parents: [i32; 2], hashes: [u32; 2]) -> Vec<u8> {
    let mut b = Vec::new();
    for n in [3, 1, 2] {
        u32b(&mut b, n);
    }
    for i in 0..2 {
        for x in [0., 0., 0., 0., 0., 0., 1., 1., 1., 1.] {
            f32b(&mut b, x);
        }
        u32b(&mut b, 4);
        b.extend(b"bone");
        b.extend((i as i32).to_le_bytes());
        b.extend(parents[i].to_le_bytes());
        u32b(&mut b, hashes[i]);
        u32b(&mut b, 0);
    }
    u32b(&mut b, 0);
    b
}
#[test]
fn rig_hierarchy_binding_and_cancellation_are_explicit() {
    let bytes = rig_fixture([-1, 0], [0x12345678, 0x87654321]);
    let rig = Rig::parse(&bytes).unwrap();
    let geom = Geometry::parse(&geom()).unwrap();
    assert_eq!(rig.bind_geometry(&geom).unwrap(), vec![[0; 4]; 3]);
    assert!(Rig::parse(&rig_fixture([1, 0], [1, 2])).is_err());
    let duplicate = Rig::parse(&rig_fixture([-1, 0], [0x12345678; 2])).unwrap();
    assert!(duplicate.bind_geometry(&geom).is_err());
    let missing = Rig::parse(&rig_fixture([-1, 0], [1, 2])).unwrap();
    assert!(missing.bind_geometry(&geom).is_err());
    for n in 0..bytes.len() {
        assert!(Rig::parse(&bytes[..n]).is_err());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancel.package");
    std::fs::write(&path, package(0, 0, b"ok", 2)).unwrap();
    let index = PackageIndex::open(&path).unwrap();
    assert!(matches!(
        index.reader().unwrap().read_resource_cancellable(
            &index.entries[0],
            ReadLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(Error::Cancelled)
    ));
}

#[test]
fn morph_attributes_and_first_stitch_records_retain_raw_identity_and_duplicates() {
    let mut chunk = b"GEOM".to_vec();
    for n in [14, 0, 4, 0, 0, 0, 3, 10] {
        u32b(&mut chunk, n);
    }
    for (usage, kind, size) in [
        (1, 1, 12),
        (2, 1, 12),
        (3, 1, 8),
        (3, 1, 8),
        (4, 2, 4),
        (5, 2, 4),
        (6, 1, 12),
        (7, 2, 4),
        (10, 2, 4),
        (77, 2, 3),
    ] {
        u32b(&mut chunk, usage);
        u32b(&mut chunk, kind);
        chunk.push(size);
    }
    for id in [-1i32, 123, 123] {
        for f in [0., 0., 0., 0., 0., 1., 0.1, 0.2, 0.3, 0.4] {
            f32b(&mut chunk, f);
        }
        chunk.extend([0; 4]);
        chunk.extend([255, 0, 0, 0]);
        for f in [1., 2., 3.] {
            f32b(&mut chunk, f);
        }
        u32b(&mut chunk, 0x80ff803f);
        chunk.extend(id.to_le_bytes());
        chunk.extend([7, 8, 9]);
    }
    u32b(&mut chunk, 1);
    chunk.push(2);
    u32b(&mut chunk, 3);
    for i in [0, 1, 2] {
        u16b(&mut chunk, i);
    }
    let tail = chunk.len();
    u32b(&mut chunk, 2);
    u32b(&mut chunk, 1);
    u32b(&mut chunk, 2);
    for f in [0.25, 0.75, 0.5, 0.625] {
        f32b(&mut chunk, f);
    }
    u32b(&mut chunk, 1);
    u32b(&mut chunk, 1);
    for f in [0.125, 0.875] {
        f32b(&mut chunk, f);
    }
    u32b(&mut chunk, 1);
    u32b(&mut chunk, 2);
    u16b(&mut chunk, 0x7123);
    for n in [0, 1, 0x12345678, 0] {
        u32b(&mut chunk, n);
    }
    let offset = chunk.len() as u32 - 16;
    chunk[8..12].copy_from_slice(&offset.to_le_bytes());
    let mut bytes = Vec::new();
    for n in [3, 0, 0, 0, 1] {
        u32b(&mut bytes, n);
    }
    bytes.extend([0; 16]);
    u32b(&mut bytes, 44);
    u32b(&mut bytes, chunk.len() as u32);
    bytes.extend(chunk);
    let geometry = Geometry::parse(&bytes).unwrap();
    assert_eq!(geometry.original, bytes);
    assert_eq!(
        geometry
            .vertices
            .iter()
            .map(|v| v.vertex_id)
            .collect::<Vec<_>>(),
        [Some(-1), Some(123), Some(123)]
    );
    assert_eq!(geometry.vertices[0].tag, Some(0x80ff803f));
    assert_eq!(geometry.vertices[0].tangent, Some([1., 2., 3.]));
    assert_eq!(geometry.vertices[0].uvs, [[0.1, 0.2], [0.3, 0.4]]);
    assert_eq!(geometry.unparsed_vertex_usages, [77]);
    assert_eq!(geometry.uv_stitches.as_ref().unwrap().len(), 2);
    assert_eq!(
        geometry.stitch_uvs(1).unwrap(),
        [[0.25, 0.75], [0.5, 0.625]]
    );
    assert!(geometry.stitch_uvs(0).unwrap().is_empty());
    assert_eq!(
        geometry.seam_stitches.as_ref().unwrap()[0].vertex_id,
        0x7123
    );
    let mut invalid = bytes.clone();
    invalid[44 + tail + 4..44 + tail + 8].copy_from_slice(&99u32.to_le_bytes());
    let refused_tail = Geometry::parse(&invalid).unwrap();
    assert!(refused_tail.palette_error.is_some());
    assert!(refused_tail.stitch_uvs(1).is_err());
    assert_eq!(refused_tail.original, invalid);
}

fn thumbnail_package(resources: &[(ResourceKey, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![0; 96];
    b[..4].copy_from_slice(b"DBPF");
    b[4..8].copy_from_slice(&2u32.to_le_bytes());
    b[8..12].copy_from_slice(&1u32.to_le_bytes());
    b[36..40].copy_from_slice(&(resources.len() as u32).to_le_bytes());
    let mut locations = Vec::new();
    for (_, bytes) in resources {
        let at = b.len();
        b.extend(bytes);
        locations.push(at);
    }
    let index_at = b.len();
    let mut index = 0u32.to_le_bytes().to_vec();
    for (i, (key, bytes)) in resources.iter().enumerate() {
        for n in [
            key.type_id,
            key.group_id,
            (key.instance_id >> 32) as u32,
            key.instance_id as u32,
            locations[i] as u32,
            bytes.len() as u32,
            bytes.len() as u32,
        ] {
            u32b(&mut index, n);
        }
    }
    b[64..68].copy_from_slice(&(index_at as u32).to_le_bytes());
    b[44..48].copy_from_slice(&(index.len() as u32).to_le_bytes());
    b.extend(index);
    b
}
fn jpeg_header(width: u16, height: u16) -> Vec<u8> {
    let mut b = vec![0xff, 0xd8, 0xff, 0xc0, 0, 11, 8];
    b.extend(height.to_be_bytes());
    b.extend(width.to_be_bytes());
    b.extend([1, 1, 0x11, 0, 0xff, 0xd9]);
    b
}
#[test]
fn thumbnail_indexes_preserve_full_keys_duplicates_and_manual_catalog_fallback() {
    use external_cas_assets::{dbpf::THUMBNAIL, scan::Provider, thumbnail::ThumbnailModelHint};
    let root = tempfile::tempdir().unwrap();
    let first = ResourceKey::new(CASP, 0x80000000, 42);
    let other_group = ResourceKey::new(CASP, 0, 42);
    let female = ResourceKey::new(THUMBNAIL, 2, 42);
    let male = ResourceKey::new(THUMBNAIL, 0x102, 42);
    let data = thumbnail_package(&[
        (first, casp(52)),
        (other_group, casp(52)),
        (female, jpeg_header(128, 256)),
        (male, jpeg_header(128, 256)),
    ]);
    std::fs::write(root.path().join("first.package"), &data).unwrap();
    std::fs::write(root.path().join("second.package"), &data).unwrap();
    let catalog = Catalog::scan_roots(
        &[root.path().to_path_buf()],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(catalog.instance_indexes_ready);
    let casps = catalog.casp_candidates(42);
    assert_eq!(casps.len(), 2);
    assert!(casps.iter().all(|c| c.providers.len() == 2));
    let thumbnails = catalog.thumbnail_candidates(42);
    assert_eq!(thumbnails.len(), 2);
    assert!(thumbnails.iter().all(|c| c.providers.len() == 2));
    assert_eq!(thumbnails[0].model_hint, ThumbnailModelHint::FemaleModel);
    assert_eq!(thumbnails[1].model_hint, ThumbnailModelHint::MaleModel);
    assert!(catalog.thumbnail_candidates(999).is_empty());
    let mut manual = Catalog::default();
    manual.by_key.insert(
        first,
        vec![Provider {
            package_index: 0,
            entry_index: 0,
        }],
    );
    manual.by_key.insert(
        female,
        vec![Provider {
            package_index: 0,
            entry_index: 1,
        }],
    );
    assert_eq!(manual.casp_candidates(42).len(), 1);
    assert_eq!(manual.thumbnail_candidates(42).len(), 1);
    manual.rebuild_instance_indexes();
    assert!(manual.instance_indexes_ready);
    assert_eq!(manual.casp_candidates(42).len(), 1);
    assert_eq!(manual.thumbnail_candidates(42).len(), 1);
}
#[test]
fn thumbnail_inspection_proves_bytes_without_selecting_group_or_source_precedence() {
    use external_cas_assets::{dbpf::THUMBNAIL, thumbnail::ThumbnailFormat};
    let root = tempfile::tempdir().unwrap();
    let key = ResourceKey::new(THUMBNAIL, 2, 42);
    let male = ResourceKey::new(THUMBNAIL, 0x102, 42);
    let data = thumbnail_package(&[(key, jpeg_header(128, 256)), (male, jpeg_header(128, 256))]);
    let first = root.path().join("first.package");
    std::fs::write(&first, &data).unwrap();
    std::fs::write(root.path().join("second.package"), &data).unwrap();
    let cancel = AtomicBool::new(false);
    let catalog = Catalog::scan_roots(&[root.path().to_path_buf()], &cancel, |_| {}).unwrap();
    let choices = catalog.thumbnail_candidates(42);
    let observed = catalog
        .inspect_thumbnails(&choices, ReadLimits::default(), &cancel)
        .unwrap();
    assert!(observed.complete);
    assert_eq!(observed.payloads.len(), 4);
    assert_eq!(observed.equivalences.len(), 1);
    assert_eq!(
        observed.equivalent_payload().unwrap().info.dimensions,
        Some([128, 256])
    );
    assert_eq!(observed.payloads[0].info.format, ThumbnailFormat::Jpeg);
    let limited = catalog
        .inspect_thumbnails(
            &choices,
            ReadLimits {
                max_resource_bytes: 1,
            },
            &cancel,
        )
        .unwrap();
    assert!(!limited.complete);
    assert!(limited.equivalent_payload().is_none());
    let changed = thumbnail_package(&[(key, jpeg_header(64, 64)), (male, jpeg_header(128, 256))]);
    std::fs::write(&first, &changed).unwrap();
    let stale = catalog
        .inspect_thumbnails(&choices, ReadLimits::default(), &cancel)
        .unwrap();
    assert!(!stale.complete);
    assert!(stale.equivalent_payload().is_none());
    let current = Catalog::scan_roots(&[root.path().to_path_buf()], &cancel, |_| {}).unwrap();
    let differing = current
        .inspect_thumbnails(
            &current.thumbnail_candidates(42),
            ReadLimits::default(),
            &cancel,
        )
        .unwrap();
    assert!(differing.complete);
    assert_eq!(differing.equivalences.len(), 2);
    assert!(differing.equivalent_payload().is_none());
    assert!(matches!(
        current.inspect_thumbnails(&[], ReadLimits::default(), &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
}
#[test]
fn design_thumbnail_fallback_is_declared_or_same_source_outfit_geometry_and_labeled() {
    use external_cas_assets::{
        dbpf::THUMBNAIL,
        thumbnail::{ThumbnailDesignIndex, ThumbnailRelationship},
    };
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let child = ResourceKey::new(CASP, 0x80000000, 1);
    let parent = ResourceKey::new(CASP, 0x80000000, 2);
    let child_bytes = casp_with_parent(52, Some(parent));
    let parent_bytes = casp(52);
    let thumb = ResourceKey::new(THUMBNAIL, 2, 2);
    std::fs::write(
        root.path().join("source.package"),
        thumbnail_package(&[
            (child, child_bytes.clone()),
            (parent, parent_bytes.clone()),
            (thumb, jpeg_header(128, 256)),
        ]),
    )
    .unwrap();
    let catalog = Catalog::scan_roots(
        &[root.path().to_path_buf()],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let child_provider = catalog.providers(child)[0];
    let parent_provider = catalog.providers(parent)[0];
    let mut designs = ThumbnailDesignIndex::new();
    designs
        .register(
            child,
            child_provider,
            Sha256::digest(&child_bytes).into(),
            &CasPart::parse(&child_bytes).unwrap(),
        )
        .unwrap();
    designs
        .register(
            parent,
            parent_provider,
            Sha256::digest(&parent_bytes).into(),
            &CasPart::parse(&parent_bytes).unwrap(),
        )
        .unwrap();
    let choices = designs.choices(&catalog, child_provider).unwrap();
    assert!(choices.exact.is_empty());
    assert_eq!(choices.design.len(), 1);
    assert_eq!(
        choices.design[0].relationship,
        ThumbnailRelationship::DeclaredParentDesign
    );
    let mut designs = ThumbnailDesignIndex::new();
    designs
        .register(
            child,
            child_provider,
            Sha256::digest(&parent_bytes).into(),
            &CasPart::parse(&parent_bytes).unwrap(),
        )
        .unwrap();
    designs
        .register(
            parent,
            parent_provider,
            Sha256::digest(&parent_bytes).into(),
            &CasPart::parse(&parent_bytes).unwrap(),
        )
        .unwrap();
    let choices = designs.choices(&catalog, child_provider).unwrap();
    assert_eq!(
        choices.design[0].relationship,
        ThumbnailRelationship::SharedOutfitGeometryDesign
    );
    let mut different = CasPart::parse(&parent_bytes).unwrap();
    different.lod0_meshes = vec![ResourceKey::new(GEOM, 0x22, 999)];
    let mut strict = ThumbnailDesignIndex::new();
    strict
        .register(child, child_provider, [0; 32], &different)
        .unwrap();
    strict
        .register(
            parent,
            parent_provider,
            [0; 32],
            &CasPart::parse(&parent_bytes).unwrap(),
        )
        .unwrap();
    assert!(
        strict
            .choices(&catalog, child_provider)
            .unwrap()
            .design
            .is_empty()
    );
}
