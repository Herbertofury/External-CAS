// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in read-only acceptance on the recovered private source-pin manifest.
//! CAS_ASSET_TEST_MANIFEST must point to a trusted local private manifest.
use external_cas_assets::*;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
#[ignore = "requires explicit private installed-source manifest; not fixture acceptance"]
fn installed_pinned_casp_geom_texture_and_rig() {
    let manifest = std::env::var_os("CAS_ASSET_TEST_MANIFEST").expect("CAS_ASSET_TEST_MANIFEST is required for explicit installed acceptance");
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    assert_eq!(j["private"], true);
    let mut indexes = Vec::new();
    for source in j["sources"].as_array().unwrap() {
        let path = source["path"].as_str().unwrap();
        let mut f = File::open(path).unwrap();
        let mut digest = Sha256::new();
        let mut b = [0u8; 1024 * 1024];
        loop {
            let n = f.read(&mut b).unwrap();
            if n == 0 {
                break;
            }
            digest.update(&b[..n]);
        }
        assert_eq!(
            format!("{:x}", digest.finalize()),
            source["sha256"].as_str().unwrap()
        );
        let index = PackageIndex::open(path).unwrap();
        assert_eq!(index.file_size, source["size"].as_u64().unwrap());
        indexes.push(index);
    }
    let casp = &j["casp"];
    let e = &indexes[0].entries[casp["ordinal"].as_u64().unwrap() as usize];
    let raw = indexes[0].read_resource(e, ReadLimits::default()).unwrap();
    assert_eq!(sha(&raw), casp["payloadSha256"].as_str().unwrap());
    let part = CasPart::parse(&raw).unwrap();
    assert!(!part.name.is_empty());
    assert!(!part.lod0_meshes.is_empty());
    let mut selected_geom = None;
    for resource in j["selectedResources"].as_array().unwrap() {
        let index = &indexes[resource["source"].as_u64().unwrap() as usize];
        let e = &index.entries[resource["ordinal"].as_u64().unwrap() as usize];
        assert_eq!(e.key.to_string(), resource["key"].as_str().unwrap());
        let raw = index.read_resource(e, ReadLimits::default()).unwrap();
        assert_eq!(sha(&raw), resource["payloadSha256"].as_str().unwrap());
        if e.key.type_id == dbpf::GEOM {
            let geom = Geometry::parse(&raw).unwrap();
            assert_eq!(geom.vertices.len(), 66);
            assert_eq!(geom.indices.len() / 3, 72);
            assert!(geom.palette_error.is_none(), "{:?}", geom.palette_error);
            selected_geom = Some(geom);
        }
    }
    let rig_bytes = std::fs::read(j["rig"]["path"].as_str().unwrap()).unwrap();
    assert_eq!(sha(&rig_bytes), j["rig"]["sha256"].as_str().unwrap());
    let rig = Rig::parse(&rig_bytes).unwrap();
    assert_eq!(rig.bones.len(), 165);
    let geom = selected_geom.unwrap();
    let bound = rig.bind_geometry(&geom).unwrap();
    assert_eq!(bound.len(), 66);
    if let Some(oracle_root) = std::env::var_os("CAS_ASSET_ORACLE_ROOT") {
        let node = std::env::var_os("CAS_ASSET_NODE").unwrap_or("node".into());
        let output = std::process::Command::new(node)
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle.mjs"))
            .arg(std::env::var_os("CAS_ASSET_TEST_MANIFEST").unwrap())
            .arg(oracle_root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for (index, value) in indexes.iter().zip(expected["indexes"].as_array().unwrap()) {
            let mut digest = Sha256::new();
            for e in &index.entries {
                digest.update(e.key.type_id.to_le_bytes());
                digest.update(e.key.group_id.to_le_bytes());
                digest.update(e.key.instance_id.to_le_bytes());
                digest.update(e.ordinal.to_le_bytes());
                digest.update(e.offset.to_le_bytes());
                digest.update(e.stored_size.to_le_bytes());
                digest.update(e.memory_size.to_le_bytes());
                digest.update(e.compression.to_le_bytes());
                digest.update(e.committed.to_le_bytes());
                digest.update([u8::from(e.extended)]);
            }
            assert_eq!(index.entries.len() as u64, value["count"].as_u64().unwrap());
            assert_eq!(
                format!("{:x}", digest.finalize()),
                value["hash"].as_str().unwrap()
            );
        }
        assert_eq!(part.name, expected["casp"]["name"].as_str().unwrap());
        assert_eq!(
            part.body_type as u64,
            expected["casp"]["bodyType"].as_u64().unwrap()
        );
        assert_eq!(
            part.diffuse.unwrap().to_string(),
            expected["casp"]["diffuse"].as_str().unwrap()
        );
        assert_eq!(
            part.lod0_meshes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            expected["casp"]["lod0"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        let mut digest = Sha256::new();
        for v in &geom.vertices {
            for n in v.position.into_iter().chain(v.normal).chain(v.uv) {
                digest.update(n.to_le_bytes());
            }
            for n in v.joints {
                digest.update(n.to_le_bytes());
            }
            digest.update(v.raw_weights);
        }
        for n in geom.indices.iter().chain(&geom.bone_palette) {
            digest.update(n.to_le_bytes());
        }
        assert_eq!(
            format!("{:x}", digest.finalize()),
            expected["geoms"][0]["hash"].as_str().unwrap()
        );
        println!(
            "recovered JS oracle matches every index locator, selected CASP roles, all GEOM position/normal/UV/joint/weight/index/palette bytes"
        );
    }
    println!(
        "real source pins and decoded payloads match; CASP {:?}; 66 vertices / 72 triangles; 165-bone authoring rig binds all vertices",
        part.name
    );
}
