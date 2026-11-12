// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use external_cas_room::*;
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() != 2 {
        return Err("usage: inspect_model explicit-provider full-TGI".into());
    }
    let index = PackageIndex::open(&a[0]).map_err(|e| e.to_string())?;
    let key: ResourceKey = a[1]
        .parse()
        .map_err(|e: external_cas_assets::Error| e.to_string())?;
    let entries: Vec<_> = index
        .matching(key)
        .filter(|entry| !entry.deleted())
        .collect();
    if entries.len() != 1 {
        return Err("model source is ambiguous".into());
    }
    let data = index
        .read_resource(
            entries[0],
            ReadLimits {
                max_resource_bytes: 32 * 1024 * 1024,
            },
        )
        .map_err(|e| e.to_string())?;
    if key.type_id == 0xc0db5ae7 {
        println!(
            "{}",
            serde_json::to_string_pretty(&parse_object_definition(&data)?)
                .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let rcol = Rcol::parse(&data)?;
    let mut models = Vec::new();
    let mut meshes = Vec::new();
    let mut materials = Vec::new();
    let mut formats = Vec::new();
    let mut skin = Vec::new();
    let mut sets = Vec::new();
    for chunk in &rcol.chunks {
        match &chunk.tag {
            b"MODL" => models.push(parse_model(&data, &rcol, chunk)?),
            b"MATD" => materials.push(parse_material(&data, chunk)?),
            b"MTST" => sets.push(parse_material_set(&data, chunk)?),
            b"VRTF" => formats
                .push(json!({"extent":chunk.extent,"format":parse_vertex_format(&data,chunk)?})),
            b"SKIN" => {
                skin.push(json!({"extent":chunk.extent,"bytes":&data[chunk.extent.clone()]}))
            }
            b"MLOD" => {
                let mlod = parse_mlod(&data, chunk)?;
                for record in &mlod.meshes {
                    let mut binding_patterns = std::collections::BTreeMap::<String, usize>::new();
                    let mut joints = Vec::new();
                    if let Some(count) = record.joint_count {
                        for i in 0..count {
                            let at = record.extent.start + 80 + i * 4;
                            joints.push(u32::from_le_bytes(data[at..at + 4].try_into().unwrap()));
                        }
                    }
                    if record.vertex_format != 0 {
                        let f = parse_vertex_format(&data, rcol.internal(record.vertex_format)?)?;
                        let buffer = rcol.internal(record.vertex_buffer)?;
                        for vertex in 0..record.vertex_count.max(0) as usize {
                            let at = buffer.extent.start
                                + 16
                                + record.stream_offset as usize
                                + vertex * f.stride;
                            let binding = f
                                .layouts
                                .iter()
                                .filter(|l| matches!(l.usage, 3 | 4))
                                .map(|l| {
                                    format!(
                                        "{}:{:?}",
                                        l.usage,
                                        &data[at + l.offset as usize
                                            ..at + l.offset as usize + l.bytes.unwrap()]
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("/");
                            *binding_patterns.entry(binding).or_default() += 1;
                        }
                    }
                    let result = (|| {
                        let format =
                            parse_vertex_format(&data, rcol.internal(record.vertex_format)?)?;
                        extract_mesh(
                            &data,
                            record,
                            format,
                            rcol.internal(record.vertex_buffer)?,
                            rcol.internal(record.index_buffer)?,
                            1_000_000,
                            2_000_000,
                        )
                    })();
                    match result{Ok(geometry)=>meshes.push(json!({"record":record,"geometry":geometry,"packed_hash":format!("{:x}",Sha256::digest(&geometry.source_vertex_bytes))})),Err(error)=>meshes.push(json!({"record":record,"error":error,"binding_patterns":binding_patterns,"joint_refs":joints,"skin_controller":u32::from_le_bytes(data[record.extent.start+72..record.extent.start+76].try_into().unwrap())}))}
                }
            }
            _ => {}
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"private":true,"provider":index.path,"key":key,"payload_sha256":format!("{:x}",Sha256::digest(&data)),"rcol":rcol,"models":models,"meshes":meshes,"materials":materials,"formats":formats,"skin":skin,"material_sets":sets})).map_err(|e|e.to_string())?);
    Ok(())
}
