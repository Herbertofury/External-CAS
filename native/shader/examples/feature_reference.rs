//! Diagnostic JSON seam; accepts explicit words/lists, never reads game files.
use external_cas_shader::{FeatureInputs, FeatureLimits, FeatureProgram};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
struct Row {
    program_words: Vec<u32>,
    renderer_ids: Vec<u32>,
    material_hashes: Vec<u32>,
    renderer_wildcard_if_empty: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Diagnostic input exceeds16MiB".into());
    }
    let rows: Vec<Row> = serde_json::from_slice(&bytes)?;
    let cancel = AtomicBool::new(false);
    let outputs: Vec<_> = rows
        .iter()
        .map(|r| {
            FeatureProgram::parse(&r.program_words, FeatureLimits::default(), &cancel).and_then(
                |program| {
                    program.evaluate(
                        FeatureInputs {
                            renderer_ids: &r.renderer_ids,
                            material_hashes: &r.material_hashes,
                            renderer_wildcard_if_empty: r.renderer_wildcard_if_empty,
                        },
                        &cancel,
                    )
                },
            )
        })
        .collect();
    serde_json::to_writer(std::io::stdout().lock(), &outputs)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
