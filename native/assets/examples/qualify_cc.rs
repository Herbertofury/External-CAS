// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit small-sample qualification. All source assets remain read-only.
use external_cas_assets::{
    dbpf::{CASP, GEOM},
    *,
};
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = std::env::args()
        .nth(1)
        .ok_or("pass a trusted private sample JSON path")?;
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(config)?)?;
    if config["private"] != true {
        return Err("sample manifest must be private".into());
    }
    let mut reports = Vec::new();
    for (kind, path) in config["samples"].as_object().ok_or("missing samples")? {
        let path = path.as_str().ok_or("sample path")?;
        let before = std::fs::read(path)?;
        let source_sha = format!("{:x}", Sha256::digest(&before));
        let package = PackageIndex::open(path)?;
        let mut reader = package.reader()?;
        let mut casps = Vec::new();
        let mut geoms = Vec::new();
        let mut textures = Vec::new();
        let mut errors = Vec::new();
        for entry in &package.entries {
            if entry.deleted() {
                continue;
            }
            if ![
                CASP, GEOM, 0x3453cf95, 0xba856c78, 0x00b2d882, 0x2f7d0004, 0x2f7d0002, 0x2f7d0006,
                0x2bc04edf,
            ]
            .contains(&entry.key.type_id)
            {
                continue;
            }
            let bytes = match reader.read_resource(entry, ReadLimits::default()) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(json!({"key":entry.key.to_string(),"stage":"payload","error":e.to_string()}));
                    continue;
                }
            };
            let payload_sha = format!("{:x}", Sha256::digest(&bytes));
            if entry.key.type_id == CASP {
                match CasPart::parse(&bytes){Ok(c)=>casps.push(json!({"key":entry.key.to_string(),"hash":payload_sha,"name":c.name,"version":c.version,"bodyType":c.body_type,"lod0":c.lod0_meshes.iter().map(ToString::to_string).collect::<Vec<_>>(),"diffuse":c.diffuse.map(|k|k.to_string()),"references":c.references.iter().map(ToString::to_string).collect::<Vec<_>>()})),Err(e)=>errors.push(json!({"key":entry.key.to_string(),"stage":"casp","error":e.to_string()}))}
            } else if entry.key.type_id == GEOM {
                match Geometry::parse(&bytes) {
                    Ok(g) => {
                        let mut digest = Sha256::new();
                        for v in &g.vertices {
                            for n in v.position.into_iter().chain(v.normal).chain(v.uv) {
                                digest.update(n.to_le_bytes());
                            }
                            for n in v.joints {
                                digest.update(n.to_le_bytes());
                            }
                            digest.update(v.raw_weights);
                        }
                        for n in g.indices.iter().chain(&g.bone_palette) {
                            digest.update(n.to_le_bytes());
                        }
                        let mut morphology = Sha256::new();
                        for v in &g.vertices {
                            morphology.update((v.uvs.len() as u32).to_le_bytes());
                            for uv in &v.uvs {
                                for n in uv {
                                    morphology.update(n.to_le_bytes());
                                }
                            }
                            morphology.update([u8::from(v.tangent.is_some())]);
                            if let Some(tangent) = v.tangent {
                                for n in tangent {
                                    morphology.update(n.to_le_bytes());
                                }
                            }
                            morphology.update([u8::from(v.tag.is_some())]);
                            if let Some(tag) = v.tag {
                                morphology.update(tag.to_le_bytes());
                            }
                            morphology.update([u8::from(v.vertex_id.is_some())]);
                            if let Some(id) = v.vertex_id {
                                morphology.update(id.to_le_bytes());
                            }
                        }
                        geoms.push(json!({"key":entry.key.to_string(),"hash":payload_sha,"decodedHash":format!("{:x}",digest.finalize()),"version":g.version,"vertices":g.vertices.len(),"triangles":g.indices.len()/3,"palette":g.bone_palette.len(),"paletteError":g.palette_error,"unknownAttributes":g.unparsed_vertex_usages,"morphAttributesHash":format!("{:x}",morphology.finalize()),"morphAttributeCounts":{"vertexIds":g.vertices.iter().filter(|v|v.vertex_id.is_some()).count(),"tags":g.vertices.iter().filter(|v|v.tag.is_some()).count(),"tangents":g.vertices.iter().filter(|v|v.tangent.is_some()).count()},"uvStitches":g.uv_stitches.as_ref().map(Vec::len),"seamStitches":g.seam_stitches.as_ref().map(Vec::len),"originalBytesPreserved":g.original==bytes}));
                    }
                    Err(e) => errors.push(
                        json!({"key":entry.key.to_string(),"stage":"geom","error":e.to_string()}),
                    ),
                }
            } else {
                let decoded = if entry.key.type_id == 0x2bc04edf {
                    match external_cas_assets::lrle::decode_lrle(&bytes, ReadLimits::default()) {
                        Ok(image) => {
                            json!({"format":if bytes.get(4..8)==Some(b"V002"){"lrle-v2"}else{"lrle-v1"},"width":image.width,"height":image.height,"mips":image.mips,"rgbaHash":format!("{:x}",Sha256::digest(&image.rgba))})
                        }
                        Err(error) => json!({"error":error.to_string()}),
                    }
                } else {
                    serde_json::Value::Null
                };
                textures.push(json!({"key":entry.key.to_string(),"hash":payload_sha,"bytes":bytes.len(),"magic":bytes.iter().take(8).map(|n|format!("{n:02X}")).collect::<String>(),"decoded":decoded}));
            }
        }
        if std::fs::read(path)? != before {
            return Err("sample source changed".into());
        }
        reports.push(json!({"category":kind,"sourceBytes":package.file_size,"sourceHash":source_sha,"entries":package.entries.len(),"casp":casps,"geometry":geoms,"textures":textures,"errors":errors}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":1,"scope":"explicit authorized read-only CC sample; not game fidelity or full-library acceptance","samples":reports})
        )?
    );
    Ok(())
}
