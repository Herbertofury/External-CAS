// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only diagnostic: package paths followed by optional --key TGI.
use external_cas_assets::{
    dbpf::{CASP, GEOM, RIG},
    *,
};
use sha2::{Digest, Sha256};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut paths = Vec::new();
    let mut key = None;
    while let Some(a) = args.next() {
        if a == "--key" {
            key = Some(args.next().ok_or("missing TGI")?.parse::<ResourceKey>()?);
        } else {
            paths.push(a);
        }
    }
    if paths.is_empty() {
        return Err("pass explicit package paths and optional --key TGI".into());
    }
    for path in paths {
        let index = PackageIndex::open(&path)?;
        let name = index.path.file_name().unwrap_or_default().to_string_lossy();
        println!(
            "{name}: {} index rows ({} CASP, {} GEOM, {} RIG)",
            index.entries.len(),
            index
                .entries
                .iter()
                .filter(|e| e.key.type_id == CASP)
                .count(),
            index
                .entries
                .iter()
                .filter(|e| e.key.type_id == GEOM)
                .count(),
            index
                .entries
                .iter()
                .filter(|e| e.key.type_id == RIG)
                .count()
        );
        let selected = index
            .entries
            .iter()
            .filter(|e| key.map(|k| e.key == k).unwrap_or(e.key.type_id == CASP))
            .take(if key.is_some() { usize::MAX } else { 3 });
        let mut reader = index.reader()?;
        for entry in selected {
            let raw = reader.read_resource(entry, ReadLimits::default())?;
            println!(
                "{} {} bytes sha256={:x}",
                entry.key,
                raw.len(),
                Sha256::digest(&raw)
            );
            match entry.key.type_id {
                CASP => {
                    let part = CasPart::parse(&raw)?;
                    println!(
                        "CASP v{} {:?} body={} lod0={} diffuse={:?}",
                        part.version,
                        part.name,
                        part.body_type,
                        part.lod0_meshes.len(),
                        part.diffuse
                    );
                }
                GEOM => {
                    let geom = Geometry::parse(&raw)?;
                    let mut bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
                    for v in &geom.vertices {
                        for i in 0..3 {
                            bounds[0][i] = bounds[0][i].min(v.position[i]);
                            bounds[1][i] = bounds[1][i].max(v.position[i]);
                        }
                    }
                    println!(
                        "GEOM v{} vertices={} triangles={} palette={} bounds={bounds:?} palette_error={:?}",
                        geom.version,
                        geom.vertices.len(),
                        geom.indices.len() / 3,
                        geom.bone_palette.len(),
                        geom.palette_error
                    );
                }
                RIG => {
                    let rig = Rig::parse(&raw)?;
                    println!("RIG bones={} chains={}", rig.bones.len(), rig.chains.len());
                }
                _ => {}
            }
        }
    }
    Ok(())
}
