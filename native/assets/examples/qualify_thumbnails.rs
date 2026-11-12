// SPDX-License-Identifier: GPL-3.0-or-later
//! Real, explicit read-only sample qualification; never writes image bytes.
use external_cas_assets::{dbpf::CASP, scan::Provider, thumbnail::*, thumbnail_decode::*, *};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::atomic::AtomicBool};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::args()
            .nth(1)
            .ok_or("private sample config required")?,
    )?)?;
    if config["private"] != true {
        return Err("private sample config required".into());
    }
    let samples = config["samples"]
        .as_object()
        .ok_or("sample mapping missing")?;
    let paths: Vec<PathBuf> = samples
        .values()
        .map(|p| p.as_str().map(PathBuf::from).ok_or("sample path"))
        .collect::<std::result::Result<_, _>>()?;
    let cancel = AtomicBool::new(false);
    let catalog = Catalog::scan_roots(&paths, &cancel, |_| {})?;
    if !catalog.issues.is_empty() {
        return Err("source discovery had errors".into());
    }
    let mut design = ThumbnailDesignIndex::new();
    let mut casps = HashMap::new();
    for (package_index, package) in catalog.packages.iter().enumerate() {
        let mut reader = package.reader()?;
        for (entry_index, entry) in package
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.deleted() && e.key.type_id == CASP)
        {
            let bytes = reader.read_resource(entry, ReadLimits::default())?;
            let part = CasPart::parse(&bytes)?;
            let provider = Provider {
                package_index,
                entry_index,
            };
            design.register(entry.key, provider, Sha256::digest(&bytes).into(), &part)?;
            casps.insert(provider, (entry.key, part));
        }
    }
    let mut reports = Vec::new();
    for (kind, path) in samples {
        let canonical = std::fs::canonicalize(path.as_str().unwrap())?;
        let package_index = catalog
            .packages
            .iter()
            .position(|p| p.path == canonical)
            .ok_or("missing package")?;
        let package = &catalog.packages[package_index];
        let mut candidates = Vec::new();
        for entry in &package.entries {
            if entry.key.type_id == external_cas_assets::dbpf::THUMBNAIL && !entry.deleted() {
                candidates.extend(catalog.thumbnail_candidates(entry.key.instance_id));
            }
        }
        let observed = catalog.inspect_thumbnails(&candidates, ReadLimits::default(), &cancel)?;
        if !observed.complete {
            return Err("thumbnail source reads failed".into());
        }
        let mut thumbs = Vec::new();
        for payload in &observed.payloads {
            let decoded = decode_pinned_thumbnail(
                &payload.bytes,
                payload.payload_sha256,
                ReadLimits::default(),
                &cancel,
            )
            .map_err(|e| {
                format!(
                    "{kind} {}: {e}; prefix={:02x?}",
                    payload.key,
                    &payload.bytes[..payload.bytes.len().min(64)]
                )
            })?;
            let alpha: Vec<u8> = decoded.rgba.chunks_exact(4).map(|p| p[3]).collect();
            thumbs.push(json!({"key":payload.key.to_string(),"entryIndex":payload.provider.entry_index,"payloadHash":hash(&payload.bytes),"bytes":payload.bytes.len(),"format":format!("{:?}",payload.info.format),"dimensions":payload.info.dimensions,"alpha":format!("{:?}",decoded.alpha),"rgbaHash":hash(&decoded.rgba),"alphaHash":hash(&alpha),"transparentPixels":alpha.iter().filter(|a|**a==0).count(),"partialAlphaPixels":alpha.iter().filter(|a|**a>0&&**a<255).count()}));
        }
        thumbs.sort_by_key(|v| v["key"].as_str().unwrap().to_owned());
        let mut parts = Vec::new();
        for (provider, (key, part)) in casps
            .iter()
            .filter(|(p, _)| p.package_index == package_index)
        {
            let choices = design.choices(&catalog, *provider)?;
            parts.push(json!({"key":key.to_string(),"outfitId":part.outfit_id,"lod0":part.lod0_meshes.iter().map(ToString::to_string).collect::<Vec<_>>(),"exact":choices.exact.iter().map(|c|json!({"key":c.key.to_string(),"providers":c.providers.len(),"modelHint":format!("{:?}",c.model_hint)})).collect::<Vec<_>>(),"design":choices.design.iter().map(|d|json!({"caspKey":d.casp_key.to_string(),"relationship":format!("{:?}",d.relationship),"keys":d.candidates.iter().map(|c|c.key.to_string()).collect::<Vec<_>>()})).collect::<Vec<_>>(),"notes":choices.notes}));
        }
        parts.sort_by_key(|v| v["key"].as_str().unwrap().to_owned());
        package.revalidate(&cancel)?;
        reports.push(json!({"category":kind,"sourceBytes":package.file_size,"sourceHash":hash(&std::fs::read(&package.path)?),"indexHash":package.index_sha256.iter().map(|n|format!("{n:02x}")).collect::<String>(),"casps":parts,"thumbnails":thumbs,"exactCaspCount":parts.iter().filter(|p|!p["exact"].as_array().unwrap().is_empty()).count(),"designOnlyCaspCount":parts.iter().filter(|p|p["exact"].as_array().unwrap().is_empty()&&!p["design"].as_array().unwrap().is_empty()).count(),"missingCaspCount":parts.iter().filter(|p|p["exact"].as_array().unwrap().is_empty()&&p["design"].as_array().unwrap().is_empty()).count()}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":1,"scope":"five explicit installed packages; source thumbnails only; no images persisted; not game appearance, app performance or complete library acceptance","samples":reports})
        )?
    );
    Ok(())
}
