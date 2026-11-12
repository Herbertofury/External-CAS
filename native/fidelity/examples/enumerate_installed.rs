// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded discovery in one explicitly named UI package. Outputs private pins.
use external_cas_assets::{PackageIndex, ReadLimits};
use external_cas_fidelity::{installed::GFX_TYPE, GfxMovie};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: enumerate_installed UI.package")?;
    let index = PackageIndex::open(path)?;
    let mut reader = index.reader()?;
    let mut result = Vec::new();
    for entry in index
        .entries
        .iter()
        .filter(|entry| entry.key.type_id == GFX_TYPE && !entry.deleted())
    {
        let bytes = reader.read_resource(
            entry,
            ReadLimits {
                max_resource_bytes: 64 * 1024 * 1024,
            },
        )?;
        result.push(match GfxMovie::inspect(&bytes){
            Ok(movie)=>json!({"key":entry.key.to_string(),"ordinal":entry.ordinal,"bytes":bytes.len(),"exporter":movie.exporter_info,"imports":movie.imports,"exports":movie.exports,"symbol_classes":movie.symbol_classes,"images":movie.external_images}),
            Err(error)=>json!({"key":entry.key.to_string(),"ordinal":entry.ordinal,"bytes":bytes.len(),"error":error.to_string()}),
        });
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"private":true,"source":index.path,"movies":result}))?
    );
    Ok(())
}
