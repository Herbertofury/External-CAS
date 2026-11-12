// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit small read-only sample; emits hashes/metadata, never source assets.
use external_cas_assets::{dbpf::CASP, *};
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("private sample config")?,
    )?)?;
    if config["private"] != true {
        return Err("private sample config required".into());
    }
    let mut samples = Vec::new();
    for (category, path) in config["samples"].as_object().ok_or("samples")? {
        let package = PackageIndex::open(path.as_str().ok_or("path")?)?;
        let mut reader = package.reader()?;
        let mut parts = Vec::new();
        for entry in package
            .entries
            .iter()
            .filter(|e| !e.deleted() && e.key.type_id == CASP)
        {
            let bytes = reader.read_resource(entry, ReadLimits::default())?;
            let part = CasPart::parse(&bytes)?;
            let metadata=part.color_sliders.map(|s|json!({"rawHash":format!("{:x}",Sha256::digest(s.raw_bytes())),"settings":s,"issues":s.validation_issues().iter().map(|i|format!("{:?}: {}",i.channel,i.reason)).collect::<Vec<_>>()}));
            parts.push(json!({"key":entry.key.to_string(),"version":part.version,"payloadHash":format!("{:x}",Sha256::digest(&bytes)),"sliders":metadata}));
        }
        package.revalidate(&std::sync::atomic::AtomicBool::new(false))?;
        samples.push(json!({"category":category,"sourceHash":format!("{:x}",Sha256::digest(std::fs::read(&package.path)?)),"parts":parts}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"samples":samples}))?
    );
    Ok(())
}
